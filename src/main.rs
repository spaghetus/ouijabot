use clap::Parser;
use dashmap::DashMap;
use memmap::Mmap;
use poise::{Context, CreateReply, Framework, serenity_prelude::ChannelId};
use std::{
    fs::File,
    iter::Peekable,
    ops::Deref,
    path::PathBuf,
    sync::{Arc, LazyLock},
};
use tracing::info;

#[derive(Parser)]
struct Args {
    #[arg(long, env = "ASKOUIJA_TOKEN")]
    pub token: String,
    #[arg(long, env = "ASKOUIJA_DICT")]
    pub dict: PathBuf,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let Args { token, dict } = Args::parse();

    let options = poise::FrameworkOptions {
        commands: vec![askouija(), tellouija(), goodbye()],
        pre_command: |ctx| {
            Box::pin(async move {
                info!(
                    "Executing command {} for {}...",
                    ctx.command().qualified_name,
                    ctx.author().name
                );
            })
        },
        // This code is run after a command if it was successful (returned Ok)
        post_command: |ctx| {
            Box::pin(async move {
                info!(
                    "Executed command {} for {}!",
                    ctx.command().qualified_name,
                    ctx.author().name
                );
            })
        },
        command_check: Some(|ctx| Box::pin(async move { Ok(true) })),

        ..Default::default()
    };
    dbg!(&options);
    let dict = unsafe { Box::new(Mmap::map(&File::open(dict).unwrap()).unwrap()) };
    let dict = std::str::from_utf8(Box::leak(dict)).unwrap();
    let dict = dict
        .lines()
        .filter(|l| {
            (l.len() > 1 || l.to_lowercase() == "a") && l.chars().all(|c| c.is_ascii_alphabetic())
        })
        .collect::<Vec<_>>()
        .into();
    let framework = poise::Framework::builder()
        .setup(
            move |ctx,
                  _ready: &poise::serenity_prelude::Ready,
                  framework: &Framework<
                (Arc<[&'static str]>, DashMap<ChannelId, Ouija>),
                color_eyre::eyre::Error,
            >| {
                Box::pin(async move {
                    println!("Logged in as {}", _ready.user.name);
                    poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                    let out: color_eyre::Result<(Arc<[&'static str]>, DashMap<ChannelId, Ouija>)> =
                        Ok((dict, DashMap::<ChannelId, Ouija>::new()));
                    out
                })
            },
        )
        .options(options)
        .build();

    let client = poise::serenity_prelude::ClientBuilder::new(
        token,
        poise::serenity_prelude::GatewayIntents::non_privileged(),
    )
    .framework(framework)
    .await;

    client.unwrap().start().await.unwrap()
}

struct Ouija {
    pub dict: Arc<[&'static str]>,
    pub message: String,
}

enum OuijaStatus {
    Accept,
    Done(Vec<&'static str>),
    Reject,
}

impl Ouija {
    pub fn new(dict: Arc<[&'static str]>) -> Self {
        Self {
            dict,
            message: String::new(),
        }
    }
    pub fn push_char(&mut self, char: char) -> OuijaStatus {
        if char == 0 as char {
            let Some(message) = self.find_valid_sequences(false).min_by_key(|v| v.len()) else {
                return OuijaStatus::Reject;
            };
            return OuijaStatus::Done(message);
        }
        self.message.push(char);
        if dbg!(self.find_valid_sequences(true).peek()).is_none() {
            self.message.pop();
            return OuijaStatus::Reject;
        }
        OuijaStatus::Accept
    }
    pub fn find_valid_sequences<'a>(
        &'a self,
        allow_trailing: bool,
    ) -> Peekable<Box<dyn Iterator<Item = Vec<&'static str>> + 'a>> {
        fn find_valid_sequences<'a>(
            dict: &'a [&'static str],
            message: &'a str,
            allow_trailing: bool,
        ) -> Box<dyn Iterator<Item = Vec<&'static str>> + 'a> {
            if message.is_empty() {
                return Box::new(vec![vec![]].into_iter());
            }
            let mut next_sequences = Box::new(
                dict.iter()
                    .copied()
                    .filter(|w| {
                        message
                            .to_ascii_lowercase()
                            .starts_with(&w.to_ascii_lowercase())
                    })
                    .flat_map(move |candidate| {
                        find_valid_sequences(dict, &message[candidate.len()..], allow_trailing).map(
                            move |mut v| {
                                v.insert(0, candidate);
                                v
                            },
                        )
                    })
                    .peekable(),
            );
            if next_sequences.peek().is_none() && allow_trailing {
                return Box::new(
                    dict.iter()
                        .copied()
                        .filter(|w| {
                            w.to_ascii_lowercase()
                                .starts_with(&message.to_ascii_lowercase())
                        })
                        .map(|v| vec![v]),
                );
            }
            next_sequences
        }
        find_valid_sequences(&self.dict, &self.message, allow_trailing).peekable()
    }
}

#[poise::command(slash_command)]
async fn askouija(
    ctx: Context<'_, (Arc<[&'static str]>, DashMap<ChannelId, Ouija>), color_eyre::eyre::Error>,
    #[description = "Question for the spirits"] question: String,
) -> color_eyre::Result<()> {
    let channel_id = ctx.channel_id();
    if ctx.data().1.contains_key(&channel_id) {
        ctx.send(
            CreateReply::default()
                .content("Channels can only fit one Ouija board at a time.")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }
    let entry = ctx
        .data()
        .1
        .insert(channel_id, Ouija::new(ctx.data().0.clone()));
    ctx.send(CreateReply::default().content(format!("New question for the spirits!\n{question}")))
        .await?;
    Ok(())
}

#[poise::command(slash_command)]
async fn tellouija(
    ctx: Context<'_, (Arc<[&'static str]>, DashMap<ChannelId, Ouija>), color_eyre::eyre::Error>,
    #[description = "Response from the spirits"] message: String,
) -> color_eyre::Result<()> {
    let channel_id = ctx.channel_id();
    let Some(mut ouija) = ctx.data().1.get_mut(&channel_id) else {
        ctx.send(
            CreateReply::default()
                .content("There isn't a board through which you can speak.")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    };
    let char = match message.as_str() {
        m if m.len() == 1 && m.chars().next().map(|c| c.is_ascii_uppercase()).unwrap() => {
            m.chars().next().unwrap()
        }
        _ => {
            ctx.send(
                CreateReply::default()
                    .content("The mortals can only receive one capital letter at a time.")
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        }
    };
    match ouija.push_char(char) {
        OuijaStatus::Accept => {
            ctx.send(CreateReply::default().content(format!("{char}")))
                .await?;
            Ok(())
        }
        OuijaStatus::Reject => {
            ctx.send(
                CreateReply::default()
                    .content("The mortals won't be able to comprehend this.")
                    .ephemeral(true),
            )
            .await?;
            Ok(())
        }
        OuijaStatus::Done(items) => {
            ctx.send(
                CreateReply::default()
                    .content(format!("The spirits have spoken!\n> {}", items.join(" "))),
            )
            .await?;
            std::mem::drop(ouija);
            ctx.data().1.remove(&channel_id);
            Ok(())
        }
    }
}

#[poise::command(slash_command)]
async fn goodbye(
    ctx: Context<'_, (Arc<[&'static str]>, DashMap<ChannelId, Ouija>), color_eyre::eyre::Error>,
) -> color_eyre::Result<()> {
    let channel_id = ctx.channel_id();
    let Some(mut ouija) = ctx.data().1.get_mut(&channel_id) else {
        ctx.send(
            CreateReply::default()
                .content("There isn't a board through which you can speak.")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    };
    match ouija.push_char(0 as char) {
        OuijaStatus::Accept => {
            unreachable!()
        }
        OuijaStatus::Reject => {
            ctx.send(
                CreateReply::default()
                    .content("The mortals won't be able to comprehend this.")
                    .ephemeral(true),
            )
            .await?;
            Ok(())
        }
        OuijaStatus::Done(items) => {
            ctx.send(
                CreateReply::default()
                    .content(format!("The spirits have spoken!\n> {}", items.join(" "))),
            )
            .await?;
            std::mem::drop(ouija);
            ctx.data().1.remove(&channel_id);
            Ok(())
        }
    }
}
