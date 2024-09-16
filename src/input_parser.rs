use std::path::Path;

use actix::{Actor, Handler, Message, System};
use dotenvy_macro::dotenv;
use glob::glob;
use log::error;

#[derive(Message)]
#[rtype(result = "Vec<String>")]
pub(crate) struct DirectoryMessage(pub String, pub String);

pub(crate) struct InputParser;

impl Actor for InputParser {
    type Context = actix::Context<Self>;
}

impl Handler<DirectoryMessage> for InputParser {
    type Result = Vec<String>; // <- Message response type

    fn handle(&mut self, msg: DirectoryMessage, _ctx: &mut actix::Context<Self>) -> Self::Result {
        const INPUT_DIRECTORY: &str = dotenv!("INPUT_DIRECTORY");

        let mut file_list: Vec<String> = Vec::new();
        // read all text files in directory and add to file_list
        let pattern = format!("{}/**/{}*_UTC.{}", INPUT_DIRECTORY, msg.0, msg.1);

        for entry in glob(&pattern).expect("Failed to read glob pattern") {
            match entry {
                Ok(path) => {
                    let path_str = &path.to_str().unwrap();
                    let filename = Path::new(path_str).file_name().unwrap().to_str().unwrap();
                    file_list.push(String::from(filename.to_owned().to_string()));
                }
                Err(e) => error!("{:?}", e),
            }
        }
        file_list
    }
}
