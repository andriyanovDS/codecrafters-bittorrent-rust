use anyhow::{Error, Result};
use serde_json;
use serde_json::{Number, Value};
use std::{env, process::exit};

fn decode_bencoded_value(encoded_value: &str) -> Result<serde_json::Value> {
    let Some(first_char) = encoded_value.chars().next() else {
        return Err(Error::msg(format!("Empty encoded value {}", encoded_value)));
    };
    match first_char {
        'i' => encoded_value[1..]
            .strip_suffix("e")
            .ok_or_else(|| Error::msg("Bencode integer must ends with e"))
            .and_then(|num| num.parse::<i64>().map_err(Error::from))
            .map(|num| Value::Number(Number::from(num))),
        _ if first_char.is_digit(10) => encoded_value
            .split_once(":")
            .and_then(|(length, string)| {
                let length = length.parse::<usize>().ok()?;
                Some(serde_json::Value::String(string[0..length].to_string()))
            })
            .ok_or_else(|| Error::msg("Invalid encoded string {encoded_value}")),
        _ => Err(Error::msg("Unexpected bencode value {encoded_value}")),
    }
}
// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: decode <bencode string>");
        exit(1);
    }
    let command = &args[1];

    if command == "decode" {
        let encoded_value = &args[2];
        let decoded_value = decode_bencoded_value(encoded_value)?;
        println!("{}", decoded_value.to_string());
    } else {
        println!("unknown command: {}", args[1])
    }
    Ok(())
}
