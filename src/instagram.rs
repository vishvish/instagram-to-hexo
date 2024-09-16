use std::borrow::Cow;

use lazy_static::lazy_static;
use regex::{Match, Regex};

pub fn match_and_replace_usernames(input: &str) -> Cow<str> {
    lazy_static! {
        static ref USERNAME_REGEX: Regex =
            Regex::new(r"\@(?P<username>[a-zA-Z][0-9a-zA-Z_]*)").unwrap();
    }
    let replacement_text: String = format!(
        "[@{}](https://www.instagram.com/{})",
        "$username", "$username"
    );
    USERNAME_REGEX.replace_all(input, replacement_text)
}

pub fn find_hashtags(input: &str) -> Vec<Match> {
    let regex = Regex::new(r"#[a-zA-Z][0-9a-zA-Z_]*").unwrap();
    let matches: Vec<_> = regex.find_iter(input).collect();
    matches
}
