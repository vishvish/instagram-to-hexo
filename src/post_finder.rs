use std::{
    any::Any,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::{exit, id},
};

use actix::{
    Actor, Context, ContextFutureSpawner, Handler, Message, ResponseActFuture, System, WrapFuture,
};
use chrono::NaiveDateTime;
use dotenvy_macro::dotenv;
use glob::glob;
use lazy_static::lazy_static;
use log::{debug, error, info};
use regex::{Captures, Regex};

// use crate::{INPUT_DIRECTORY, OUTPUT_DIRECTORY};

#[derive(Message)]
#[rtype(result = "Result<HashMap<String, String>, std::io::Error>")]
pub(crate) struct PostFinderMessage(pub String);

pub(crate) struct PostFinder;

impl Actor for PostFinder {
    type Context = Context<Self>;
}

impl Handler<PostFinderMessage> for PostFinder {
    type Result = Result<HashMap<String, String>, std::io::Error>; // <- Message response type

    fn handle(&mut self, msg: PostFinderMessage, _ctx: &mut Context<Self>) -> Self::Result {
        let dd = extract_datetime_from_name(msg.0.as_str());

        // TODO move this into another actor or somewhere else
        create_output_directories(&dd);
        Ok(dd)
    }
}

fn extract_datetime_from_name(input: &str) -> HashMap<String, String> {
    let mut datetime_dictionary: HashMap<String, String> = HashMap::with_capacity(5);
    let file_path = input;

    lazy_static! {
        static ref DATETIME_REGEX: Regex = Regex::new(r"(?P<dt>.*)(_UTC)").unwrap();
    }
    debug!("attempting to match: {:?}", file_path);
    let captures = DATETIME_REGEX.captures(file_path);
    match captures {
        None => {
            error!("Error: {:?}", "No match");
            // exit(0);
        }
        Some(_) => {
            debug!("match: {:?}", captures);
        }
    }
    let post_utc_time = NaiveDateTime::parse_from_str(&captures.unwrap()[1], "%Y-%m-%d_%H-%M-%S");

    match post_utc_time {
        Ok(dt) => {
            debug!("post_utc_time: {:?}", dt);
            debug!("date: {:?}", dt.date());
            debug!("time: {:?}", dt.time());
            let formatted = format!("{}", dt.format("%Y/%m/%d/"));
            debug!("formatted: {}", formatted);
        }
        Err(e) => {
            error!("Error: {:?}", e);
            panic!("Error: {:?}", e);
        }
    }
    let dt = post_utc_time.unwrap();
    datetime_dictionary.insert("post_path".to_string(), input.to_string());
    datetime_dictionary.insert(
        "title".to_string(),
        format!("{}", dt.format("%A, %B %e, %Y")),
    );
    datetime_dictionary.insert("date".to_string(), format!("{}", dt.format("%Y-%m-%d")));
    datetime_dictionary.insert("time".to_string(), format!("{}", dt.format("%H:%M %p")));
    datetime_dictionary.insert(
        "output_path".to_string(),
        format!("{}", dt.format("%Y/%m/%d")),
    );
    datetime_dictionary
}

fn create_output_directories(datetime_dictionary: &HashMap<String, String>) {
    const OUTPUT_DIRECTORY: &str = dotenv!("OUTPUT_DIRECTORY");

    let img_output_path: String = format!(
        "{}/{}/{}/{}/",
        OUTPUT_DIRECTORY,
        "img",
        "instagram",
        datetime_dictionary.get("output_path").unwrap()
    );
    let markdown_output_path: String = format!(
        "{}/{}/",
        OUTPUT_DIRECTORY,
        datetime_dictionary.get("output_path").unwrap()
    );
    fs::create_dir_all(img_output_path.clone()).expect("Unable to create img directory");
    fs::create_dir_all(markdown_output_path.clone()).expect("Unable to create markdown directory");
}
