#![allow(unused)]

use std::{
    collections::HashSet,
    env, fs, iter,
    iter::Sum,
    path::Path,
    process::{exit, id},
};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::hash::Hash;
use std::io::Error;
use std::iter::{Flatten, Map};
use std::path::PathBuf;

use actix::{Actor, Addr, fut::result, Handler, MailboxError, Message, SyncArbiter, System};
use actix::dev::Request;
use actix_rt::Arbiter;
use chrono::{Datelike, DateTime, format::parse, NaiveDateTime, Utc, Weekday};
use dotenvy::dotenv;
use dotenvy_macro::dotenv;

use env_logger::{Builder, Target};
use future::try_join_all;
use futures::future;
use glob::glob;
use lazy_static::lazy_static;
use log::{debug, error, info};
use num_cpus::get;
use photon_rs::{
    multiple::watermark,
    native::{open_image, save_image},
    PhotonImage,
    transform::crop,
    transform::resize,
    transform::SamplingFilter,
};
use rayon::prelude::*;
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use tera::{Context, Tera};

use input_parser::{DirectoryMessage, InputParser};

use crate::{
    asset_finder::{AssetFinder, AssetMessage},
    media_processor::{MediaMessage, MediaProcessor},
    post_finder::{PostFinder, PostFinderMessage},
};
use crate::post_actor::{PostActor, PostMessage};

mod asset_finder;
mod input_parser;
mod instagram;
mod media_processor;
mod post_actor;
mod post_finder;

#[derive(Serialize, Debug)]
struct Post {
    title: String,
    thumbnail_image: String,
    date: String,
    time_heading: String,
    categories: Vec<String>,
    tags: Vec<String>,
    heading: String,
    text: String,
    images: Vec<String>,
    filename: String,
}

impl Post {
    pub(crate) fn default() -> Post {
        Post {
            title: "".to_string(),
            thumbnail_image: "".to_string(),
            date: "".to_string(),
            time_heading: "".to_string(),
            categories: Vec::new(),
            tags: Vec::new(),
            heading: "".to_string(),
            text: "".to_string(),
            images: Vec::new(),
            filename: "".to_string(),
        }
    }
}

struct InMemoryStore {
    lines: VecDeque<String>,
}

impl InMemoryStore {
    fn new() -> Self {
        InMemoryStore {
            lines: VecDeque::new(),
        }
    }

    fn add_line(&mut self, line: String) {
        self.lines.push_back(line);
    }

    fn read_all_lines(&self) -> Vec<String> {
        self.lines.iter().cloned().collect()
    }
}

#[actix::main]
async fn main() {
    dotenv().expect(".env file not found");
    env_logger::init();
    run_processor().await;
}

async fn run_processor() {
    let arbiters = create_arbiters();

    let mut post_store = InMemoryStore::new();
    let mut asset_store = InMemoryStore::new();

    let res = read_files().await; // <- send message and get future for result

    // res will contain a list of files ending in .txt
    match res {
        Ok(result) => {
            for (index, filename) in result.clone().iter().enumerate() {
                post_store.add_line(filename.to_string());
            }

            // Step 1
            let post_res: Result<Vec<Result<HashMap<String, String>, Error>>, MailboxError> =
                find_posts(&arbiters, post_store.read_all_lines()).await;

            // Step 2
            let asset_list: Result<
                Vec<Result<(HashMap<String, String>, Vec<String>), Error>>,
                MailboxError,
            > = find_media(&arbiters, post_res).await;

            // Step 3

            // Unwrap the outer Result
            if let Ok(asset_results) = asset_list {
                // Collect all the inner Results into a single Vec
                let mut all_assets: Vec<String> = Vec::new();
                for asset_result in asset_results {
                    // Unwrap each inner Result
                    if let Ok((_, assets)) = asset_result {
                        // Add the assets to the all_assets Vec
                        all_assets.extend(assets);
                    }
                }
                for asset in all_assets.iter() {
                    let asset_filename = Path::new(&asset).file_name().unwrap().to_str().unwrap();
                    asset_store.add_line(asset_filename.to_string());
                }
                // Now asset_store contains all the assets from all the Results
            }
            // debug!("all_assets: {:?}", asset_store.read_all_lines());
            process_media(&arbiters, asset_store.read_all_lines()).await;

            // Step 4
            render_posts(
                &arbiters,
                post_store.read_all_lines(),
                asset_store.read_all_lines(),
            )
            .await;

            info!("Total Files: {}", &result.len());
        }
        Err(e) => {
            error!("Communication to the actor has failed");
            exit(0);
        }
    }
}

// create one arbiter per cpu core
fn create_arbiters() -> Vec<Arbiter> {
    let arbiters: Vec<Arbiter> = Map::collect((0..40).map(|_| Arbiter::new()));
    // let arbiters: Vec<Arbiter> = Map::collect((0..num_cpus::get()).map(|_| Arbiter::new()));
    debug!("# of arbs: {:?}", arbiters.len());
    arbiters
}

// read all the text files in the input directory
// turn all the file names into a vector of strings
async fn read_files() -> Result<Vec<String>, MailboxError> {
    let input_addr: Addr<InputParser> = InputParser.start();
    const TEST_INPUT: &str = dotenv!("TEST_INPUT");

    debug!("TEST_INPUT: {}", TEST_INPUT);

    let res: Result<Vec<String>, MailboxError> = input_addr
        .send(DirectoryMessage(TEST_INPUT.to_owned(), "txt".to_owned()))
        .await; // <- send message and get future for result
    res
}

async fn render_posts(
    arbiters: &Vec<Arbiter>,
    posts: Vec<String>,
    assets: Vec<String>,
) -> Result<Vec<Result<(), Error>>, MailboxError> {
    // we have the list of posts and assets, let's render the posts
    let mut post_futs: Vec<Request<PostActor, PostMessage>> = Vec::new();

    for (index, file_name) in posts.iter().enumerate() {
        debug!("Render Post #{} of {}", index, posts.len());
        // choose an arbiter to use
        let arb: &Arbiter = &arbiters[index % arbiters.len()];
        debug!("arb: {:?}", arb);

        let post_stem = Path::new(&file_name).file_stem().unwrap().to_str().unwrap();
        let post_assets = find_lines_starting_with(assets.clone(), post_stem);

        debug!("Found assets for post {}: {:?}", post_stem, post_assets);

        let post_renderer: Addr<PostActor> =
            PostActor::start_in_arbiter(&arb.handle(), |_ctx| PostActor);
        post_futs.push(post_renderer.send(PostMessage(file_name.to_string(), post_assets)));
        debug!("sent path to PostActor");
    }

    try_join_all(post_futs).await
}

fn find_lines_starting_with(lines: Vec<String>, start: &str) -> Vec<String> {
    lines
        .into_iter()
        .filter(|line| line.starts_with(start))
        .collect()
}

async fn process_media(
    arbiters: &Vec<Arbiter>,
    asset_list: Vec<String>,
) -> Result<Vec<Result<HashMap<String, String>, Error>>, MailboxError> {
    // we have a complete list of media files, let's process them

    let mut file_futs: Vec<Request<MediaProcessor, MediaMessage>> = Vec::new();

    for (index, filepath) in asset_list.iter().enumerate() {
        // check file extensions and only send images to be processed
        let mut path: PathBuf = PathBuf::from(&filepath);
        let filename = &path.file_name().unwrap().to_str().unwrap();
        let file_extension = Path::new(&filename).extension().unwrap().to_str().unwrap();
        if file_extension == "mp4" {
            debug!("process_media: skipping video");
            continue;
        }

        // choose an arbiter to use
        let arb: &Arbiter = &arbiters[index % arbiters.len()];
        let asset_addr: Addr<MediaProcessor> =
            MediaProcessor::start_in_arbiter(&arb.handle(), |_ctx| MediaProcessor);
        debug!("process_media: {}", filepath);
        file_futs.push(asset_addr.send(MediaMessage(filepath.to_string())));
    }
    let file_res: Result<Vec<Result<HashMap<String, String>, Error>>, MailboxError> =
        try_join_all(file_futs).await;

    debug!("process_media file_res: {:?}", file_res);
    file_res
}

// async fn build_post(arbiters: &Vec<Arbiter>, raw_post_file: String, media_files: Vec<String>) {
//     info!("txt_to_markdown: {:?}", media_files);
//     for (index, data) in media_files.iter().enumerate() {
//         info!("data: {}", data);
//     }
//     // convert text data to an intermediate object
//
//     // TODO: implement templating markdown
// }

async fn find_media(
    arbiters: &Vec<Arbiter>,
    post_res: Result<Vec<Result<HashMap<String, String>, Error>>, MailboxError>,
) -> Result<Vec<Result<(HashMap<String, String>, Vec<String>), Error>>, MailboxError> {
    // we have the posts, let's get the list of media files
    let mut asset_futs: Vec<Request<AssetFinder, AssetMessage>> = Vec::new();
    for (index, data) in post_res.unwrap().iter().enumerate() {
        // choose an arbiter to use
        let arb: &Arbiter = &arbiters[index % arbiters.len()];
        debug!("arb: {:?}", arb);

        // this dictionary contains the tokenized date/time information
        let datetime_dictionary: HashMap<String, String> = data.as_ref().unwrap().clone();

        let asset_addr: Addr<AssetFinder> =
            AssetFinder::start_in_arbiter(&arb.handle(), |_ctx| AssetFinder);
        asset_futs.push(asset_addr.send(AssetMessage(datetime_dictionary)));
        debug!("sent path to AssetFinder");
    }
    let asset_result: Result<
        Vec<Result<(HashMap<String, String>, Vec<String>), Error>>,
        MailboxError,
    > = try_join_all(asset_futs).await;

    // The result is a tuple containing the original dictionary and the list of media files

    // we want to flatten the list of media files and then return an updated result

    // match asset_result.unwrap() {
    //     Ok((dict, media)) => {
    //         // You can now use hashmap and vec here
    //         // println!("{:?}", hashmap);
    //         // println!("{:?}", vec);
    //         let asset_list_interim = media.unwrap().into_iter().collect();
    //         let asset_list = Flatten::collect(asset_list_interim.unwrap().into_iter().flatten());
    //         (dict, asset_list)
    //     }
    //     Err(e) => {
    //         // Handle the error here
    //         error!("Error: {:?}", e)
    //     }
    // }

    // TODO: optimize this flattening
    // let asset_list_interim: Result<Vec<_>, _> = asset_result.unwrap().into_iter().collect();
    // let asset_list: Vec<String> =
    //     Flatten::collect(asset_list_interim.unwrap().into_iter().flatten());
    //
    // debug!("flattened: {:?}", asset_list);
    // asset_list;
    asset_result
}
/*
Finding posts is about sending each path as line to a PostFinder actor which matches the date/time information in the filename and returns a dictionary of the date/time information. This information is used to create an output directory string and path from the date/time information. The output directory is created and the dictionary is returned.
 */
async fn find_posts(
    arbiters: &Vec<Arbiter>,
    result: Vec<String>,
) -> Result<Vec<Result<HashMap<String, String>, Error>>, MailboxError> {
    // we have the list of files, let's find the posts
    let mut post_futs: Vec<Request<PostFinder, PostFinderMessage>> = Vec::new();
    for (index, file_name) in result.iter().enumerate() {
        // choose an arbiter to use
        let arb: &Arbiter = &arbiters[index % arbiters.len()];
        debug!("arb: {:?}", arb);

        let post_addr: Addr<PostFinder> =
            PostFinder::start_in_arbiter(&arb.handle(), |_ctx| PostFinder);
        post_futs.push(post_addr.send(PostFinderMessage(file_name.to_string())));
        debug!("sent path to PostFinder");
    }
    let post_res: Result<Vec<Result<HashMap<String, String>, Error>>, MailboxError> =
        try_join_all(post_futs).await;

    debug!("posts: {:?}", post_res);
    post_res
}