use std::collections::HashSet;
use std::iter::Peekable;

use std::sync::Arc;

#[derive(Clone, Debug)]
pub(crate) struct Ouija {
	pub dict: Arc<[&'static str]>,
	pub message: String,
	pub guesses: Vec<&'static str>,
	pub accepting: bool,
}

pub(crate) enum OuijaStatus {
	Accept,
	Done(Vec<&'static str>),
	Reject,
}

impl Ouija {
	pub fn new(dict: Arc<[&'static str]>) -> Self {
		Self {
			guesses: dict.to_vec(),
			dict,
			message: String::new(),
			accepting: false,
		}
	}
	pub fn push_char(&mut self, char: char) -> OuijaStatus {
		if char == 0 as char {
			if !self.accepting {
				return OuijaStatus::Reject;
			}
			let Some(message) = self.find_valid_sequences(false).min_by_key(|v| v.len()) else {
				return OuijaStatus::Reject;
			};
			return OuijaStatus::Done(message);
		}
		if !self.guesses.iter().any(|g| g.starts_with(char)) {
			return OuijaStatus::Reject;
		}
		self.accepting = false;
		self.guesses.retain_mut(|g| {
			if !g.starts_with(char) {
				return false;
			}
			*g = &g[1..];
			if !g.is_empty() {
				return true;
			}
			self.accepting = true;
			false
		});
		if self.accepting {
			self.guesses.extend(self.dict.iter().copied());
		}
		self.message.push(char);
		OuijaStatus::Accept
	}
	// pub fn push_char(&mut self, char: char) -> OuijaStatus {
	// 	if char == 0 as char {
	// 		let Some(message) = self.find_valid_sequences(false).min_by_key(|v| v.len()) else {
	// 			return OuijaStatus::Reject;
	// 		};
	// 		return OuijaStatus::Done(message);
	// 	}
	// 	self.message.push(char);
	// 	if dbg!(self.find_valid_sequences(true).peek()).is_none() {
	// 		self.message.pop();
	// 		return OuijaStatus::Reject;
	// 	}
	// 	OuijaStatus::Accept
	// }
	pub fn find_valid_sequences<'a>(
		&'a self,
		allow_trailing: bool,
	) -> Peekable<Box<dyn Iterator<Item = Vec<&'static str>> + 'a>> {
		pub(crate) fn find_valid_sequences<'a>(
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
	pub fn legal_next_characters(&self) -> HashSet<char> {
		self.guesses
			.iter()
			.flat_map(|g| g.chars().next().map(|c| c.to_ascii_uppercase()))
			.collect::<HashSet<_>>()
	}
}
