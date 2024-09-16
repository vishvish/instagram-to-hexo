# Instagram to Hexo

## Turning my Instagram posts (https://www.instagram.com/koothooloo/) into Markdown for my Hexo site (https://vishvish.com/)

Two steps are required:

1. Download latest posts as text + images/video from Instagram.
2. Parse through images, watermarking and sizing them, and pushing text into a Markdown template.

The first involves using the Python Instaloader package: https://instaloader.github.io/

The second involves running the new Rust program I made with Actors in order to learn the Actix library. I have quite enjoyed using it and the program uses all the cores on my MBP and pegs the CPU around 800-850% which is marvellous. It replaces a hacked-together old Ruby script.

This is NOT a serious piece of software, there is nothing riding on it, I merely hacked it together just for kicks. There's loads of room for changing how it works or optimizing the code, but real work and real life gets in the way!

### Step 1

To get the latest images:

```
instaloader --fast-update koothooloo
```

### Step 2

Build the rust app: `cargo build -r` and copy it back to the root folder `mv target/release/vv-instagram ./`

and run it `./vv-instagram`
