use clap::Parser;
use dashmap::DashMap;
use memmap::Mmap;
use poise::{
	Context, CreateReply, Framework,
	serenity_prelude::{self, AutocompleteChoice, ChannelId, json::Value},
};
use std::{
	fs::File,
	path::PathBuf,
	sync::Arc,
	time::{Duration, Instant},
};
use tracing::info;

#[derive(Parser)]
struct Args {
	#[arg(long, env = "ASKOUIJA_TOKEN")]
	pub token: String,
	#[arg(long, env = "ASKOUIJA_DICT")]
	pub dict: PathBuf,
}

struct Data {
	pub dictionary: Arc<[&'static str]>,
	pub boards: DashMap<ChannelId, Board>,
}

struct Board {
	pub last_updated: Instant,
	pub ouija: ouija::Ouija,
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
			      framework: &Framework<Data, color_eyre::eyre::Error>| {
				Box::pin(async move {
					println!("Logged in as {}", _ready.user.name);
					poise::builtins::register_globally(ctx, &framework.options().commands).await?;
					let out: color_eyre::Result<Data> = Ok(Data {
						dictionary: dict,
						boards: DashMap::<ChannelId, Board>::new(),
					});
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

mod ouija;

#[poise::command(slash_command)]
async fn askouija(
	ctx: Context<'_, Data, color_eyre::eyre::Error>,
	#[description = "Question for the spirits"] question: String,
) -> color_eyre::Result<()> {
	let channel_id = ctx.channel_id();
	if ctx
		.data()
		.boards
		.get(&channel_id)
		.map(|b| b.last_updated.elapsed() > Duration::from_secs(60 * 10))
		.unwrap_or(false)
	{
		ctx.send(
			CreateReply::default()
				.content("Channels can only fit one Ouija board at a time.")
				.ephemeral(true),
		)
		.await?;
		return Ok(());
	}
	let entry = ctx.data().boards.insert(
		channel_id,
		Board {
			ouija: ouija::Ouija::new(ctx.data().dictionary.clone()),
			last_updated: Instant::now(),
		},
	);
	ctx.send(CreateReply::default().content(format!("New question for the spirits!\n{question}")))
		.await?;
	Ok(())
}

async fn autocomplete_ouija(
	ctx: Context<'_, Data, color_eyre::eyre::Error>,
	partial: &str,
) -> Box<dyn Iterator<Item = char> + Send + Sync> {
	let Some(board) = ctx.data().boards.get(&ctx.channel_id()) else {
		return Box::new(std::iter::empty());
	};
	let valid = board.ouija.legal_next_characters();
	match partial.len() {
		0 => Box::new(valid.into_iter()),
		1 => {
			let char = partial.chars().next().unwrap();
			if valid.contains(&char) {
				Box::new(std::iter::once(char))
			} else {
				Box::new(std::iter::empty())
			}
		}
		_ => Box::new(std::iter::empty()),
	}
}

#[poise::command(slash_command)]
async fn tellouija(
	ctx: Context<'_, Data, color_eyre::eyre::Error>,
	#[description = "Response from the spirits"]
	#[autocomplete = "autocomplete_ouija"]
	char: char,
) -> color_eyre::Result<()> {
	let channel_id = ctx.channel_id();
	let Some(mut board) = ctx.data().boards.get_mut(&channel_id) else {
		ctx.send(
			CreateReply::default()
				.content("There isn't a board through which you can speak.")
				.ephemeral(true),
		)
		.await?;
		return Ok(());
	};
	board.last_updated = Instant::now();
	if !char.is_ascii_uppercase() {
		ctx.send(
			CreateReply::default()
				.content("The mortals can only receive capital letters.")
				.ephemeral(true),
		)
		.await?;
		return Ok(());
	}
	match board.ouija.push_char(char) {
		ouija::OuijaStatus::Accept => {
			ctx.send(CreateReply::default().content(format!("{char}")))
				.await?;
			Ok(())
		}
		ouija::OuijaStatus::Reject => {
			ctx.send(
				CreateReply::default()
					.content("The mortals won't be able to comprehend this.")
					.ephemeral(true),
			)
			.await?;
			Ok(())
		}
		ouija::OuijaStatus::Done(items) => {
			ctx.send(
				CreateReply::default()
					.content(format!("The spirits have spoken!\n> {}", items.join(" "))),
			)
			.await?;
			std::mem::drop(board);
			ctx.data().boards.remove(&channel_id);
			Ok(())
		}
	}
}

#[poise::command(slash_command)]
async fn goodbye(ctx: Context<'_, Data, color_eyre::eyre::Error>) -> color_eyre::Result<()> {
	let channel_id = ctx.channel_id();
	let Some(mut board) = ctx.data().boards.get_mut(&channel_id) else {
		ctx.send(
			CreateReply::default()
				.content("There isn't a board through which you can speak.")
				.ephemeral(true),
		)
		.await?;
		return Ok(());
	};
	match board.ouija.push_char(0 as char) {
		ouija::OuijaStatus::Accept => {
			unreachable!()
		}
		ouija::OuijaStatus::Reject => {
			ctx.send(
				CreateReply::default()
					.content("The mortals won't be able to comprehend this.")
					.ephemeral(true),
			)
			.await?;
			Ok(())
		}
		ouija::OuijaStatus::Done(items) => {
			ctx.send(
				CreateReply::default()
					.content(format!("The spirits have spoken!\n> {}", items.join(" "))),
			)
			.await?;
			std::mem::drop(board);
			ctx.data().boards.remove(&channel_id);
			Ok(())
		}
	}
}
