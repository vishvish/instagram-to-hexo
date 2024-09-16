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
use lazy_static::lazy_static;
use log::{debug, error, info};
use regex::Regex;
use tera::Tera;

use crate::{instagram, Post};

#[derive(Message)]
#[rtype(result = "Result<(), std::io::Error>")]

pub(crate) struct PostMessage(pub String, pub Vec<String>);

pub(crate) struct PostActor;

impl Actor for PostActor {
    type Context = Context<Self>;

    // fn started(&mut self, _ctx: &mut Self::Context) {
    //     info!("PostActor is started");
    //     // System::current().stop(); // <- stop system
    // }
    //
    // fn stopped(&mut self, _ctx: &mut Self::Context) {
    //     info!("PostActor is stopped");
    // }
}

impl Handler<PostMessage> for PostActor {
    type Result = Result<(), std::io::Error>; // <- Message response type

    fn handle(&mut self, msg: PostMessage, _ctx: &mut Context<Self>) -> Self::Result {
        const OUTPUT_DIRECTORY: &str = dotenv!("OUTPUT_DIRECTORY");

        debug!("Going to render post: {}", msg.0.as_str());
        debug!("Post {} has assets: {:?}", msg.0.as_str(), msg.1);

        let post = convert_post(msg.0.as_str(), msg.1);
        let output_path = post.filename.clone();
        info!("Post: {:?}", post);
        let rendered = render_template(post);
        write_file(&output_path, rendered);
        Ok(())
    }
}

fn render_template(post: Post) -> String {
    lazy_static! {
        static ref TEMPLATES: Tera = {
            let mut tera = match Tera::new("templates/**/*.md") {
                Ok(t) => t,
                Err(e) => {
                    println!("Parsing error(s): {}", e);
                    exit(1);
                }
            };
            tera.autoescape_on(vec![".html", ".sql"]);
            // tera.register_filter("do_nothing", do_nothing_filter);
            tera
        };
    }

    match &tera::Context::from_serialize(&post) {
        Ok(t) => debug!("Context: {:?}", &tera::Context::from_serialize(&post)),
        Err(e) => error!("Error: {:?}", e),
    }

    let output = TEMPLATES.render(
        "001_post.md",
        &tera::Context::from_serialize(&post).unwrap(),
    );

    debug!("Output: {:?}", output);
    return output.unwrap();
}

fn write_file(output_path: &str, rendered: String) -> Result<(), std::io::Error> {
    const OUTPUT_DIRECTORY: &str = dotenv!("OUTPUT_DIRECTORY");
    let output_file_path = format!("{}/{}", OUTPUT_DIRECTORY, output_path);
    debug!("output_file_path: {}", output_file_path);
    // fs::create_dir_all(Path::new(&output_file_path).parent().unwrap())?;
    fs::write(output_file_path, rendered).expect("Unable to write file");
    Ok(())
}

fn convert_post(post_name: &str, asset_list: Vec<String>) -> Post {
    const INPUT_DIRECTORY: &str = dotenv!("INPUT_DIRECTORY");

    // read post text file
    let post_file_path = format!("{}/{}", INPUT_DIRECTORY, post_name);

    // we need the datetime portion of the file path
    debug!("post_file_path: {}", post_file_path);

    let mut post_file_contents: String =
        fs::read_to_string(&post_file_path).expect("Something went wrong reading the file");

    post_file_contents = instagram::match_and_replace_usernames(&post_file_contents).to_string();

    let hashtags_result = instagram::find_hashtags(&post_file_contents);

    let tags: Vec<String> = hashtags_result
        .iter()
        .map(|m| m.as_str().to_string())
        .map(|s| s.strip_prefix("#").unwrap_or(&s).to_string())
        .collect();

    // if title_prefix is not blank, add a space and a pipe at the end
    let mut title_prefix = match tags.get(0) {
        Some(s) => format!("{} | ", s.as_str()),
        None => String::from(""),
    };

    // we need a filename prefix, taken from the post text
    if title_prefix.is_empty() {
        // get the first two words of the post
        let words: Vec<&str> = post_file_contents.split_whitespace().collect();
        let first_two_words: Vec<String> = words
            .into_iter()
            .take(2)
            .map(|word| {
                word.chars()
                    .filter(|ch| !ch.is_ascii_punctuation())
                    .collect()
            })
            .collect();
        title_prefix = first_two_words.join(" ")
    } else {
        // get the first four words of the post plus the first hashtag, put into a Vec
        // let words: Vec<&str> = post_file_contents.split_whitespace().collect();
        title_prefix = tags[0].clone();
    }

    // we want to parameterize the prefix and replace special characters with pretty url-friendly characters
    title_prefix = unidecode::unidecode(&title_prefix).to_lowercase();
    let chars_to_replace = vec![
        ',', ' ', '!', '.', ':', '?', '#', '@', '(', ')', '=', '\'', '"',
    ];

    for ch in chars_to_replace {
        title_prefix = title_prefix.replace(ch, "-");
    }
    // remove any double hyphens
    title_prefix = title_prefix.replace("--", "-");

    // these are the useful bits of the file path
    let meta_headings = make_headings_from_filepath(post_file_path.to_string());

    let file_stem = Path::new(post_name).file_stem().unwrap().to_str().unwrap();

    // if there are no tags, leave the title alone else prefix with the first tag and a space
    let title: String;

    if tags.len() > 0 {
        title = format!("{} | {}", tags[0], meta_headings.0);
    } else {
        title = meta_headings.0;
    }
    // prepend the output path to each asset
    let asset_paths: Vec<String> = asset_list
        .iter()
        .map(|s| format!("/img/instagram/{}/{}", meta_headings.4, s))
        .collect();

    let thumbnail_image = {
        format!(
            "/img/instagram/{}/{}{}{}",
            meta_headings.4,
            Path::new(&asset_paths[0])
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap(),
            "_thumb.",
            Path::new(&asset_paths[0])
                .extension()
                .unwrap()
                .to_str()
                .unwrap()
        )
    };

    // render the markdown template
    let post = Post {
        title,
        thumbnail_image,
        date: meta_headings.1,
        time_heading: meta_headings.3,
        categories: vec![String::from("instagram")],
        tags,
        heading: meta_headings.2,
        text: post_file_contents,
        images: asset_paths,
        filename: format!(
            "{}/{}-{}.md",
            meta_headings.4,
            title_prefix.clone(),
            file_stem
        ),
    };
    post
}

pub(crate) fn make_headings_from_filepath(path: String) -> (String, String, String, String, String) {
    let dt = get_datetime_from_string(&path);
    let title = format!("{}", dt.format("%A, %B %e, %Y"));
    let date = format!("{}", dt.format("%Y-%m-%d"));
    let time_12_hour = format!("{}", dt.format("%l:%M %p"));
    let time_24_hour = format!("{}", dt.format("%H:%M:%S"));
    let output_path = format!("{}", dt.format("%Y/%m/%d"));
    (
        title.to_string(),
        date.to_string(),
        time_12_hour.to_string(),
        time_24_hour.to_string(),
        output_path.to_string(),
    )
}

fn get_datetime_from_string(file_path: &str) -> NaiveDateTime {
    lazy_static! {
        static ref DATETIME_REGEX: Regex = Regex::new(r"(?P<dt>.*)(_UTC)").unwrap();
    }
    let filename = Path::new(file_path).file_name().unwrap().to_str().unwrap();
    let captures = DATETIME_REGEX.captures(filename).unwrap();
    let post_utc_time = NaiveDateTime::parse_from_str(&captures[1], "%Y-%m-%d_%H-%M-%S");

    match post_utc_time {
        Ok(dt) => {
            debug!("post_utc_time: {:?}", dt);
            debug!("date: {:?}", dt.date());
            debug!("time: {:?}", dt.time());
        }
        Err(e) => {
            error!("Error: {:?}", e);
            panic!("Error: {:?}", e);
        }
    }
    post_utc_time.unwrap()
}
