use serde_json;
use std::{env, process::exit};

// Available if you need it!
// use serde_bencode

fn decode_bencoded_value(encoded_value: &str) -> serde_json::Value {
    encoded_value
        .split_once(":")
        .and_then(|(length, string)| {
            let length = length.parse::<usize>().ok()?;
            Some(serde_json::Value::String(string[0..length].to_string()))
        })
        .expect("Unexpected type {encoded_value:?}")
}
// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: decode <bencode string>");
        exit(1);
    }
    let command = &args[1];

    if command == "decode" {
        let encoded_value = &args[2];
        let decoded_value = decode_bencoded_value(encoded_value);
        println!("{}", decoded_value.to_string());
    } else {
        println!("unknown command: {}", args[1])
    }
}
