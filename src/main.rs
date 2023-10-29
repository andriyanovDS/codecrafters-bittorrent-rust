use anyhow::{Error, Result};
use serde_json::{Map, Number, Value};
use std::{env, process::exit};

fn decode_bencoded_value(encoded_value: &str) -> Result<(&str, Value)> {
    let Some(first_char) = encoded_value.chars().next() else {
        return Err(Error::msg(format!("Empty encoded value {}", encoded_value)));
    };
    match first_char {
        'i' => encoded_value[1..]
            .split_once('e')
            .ok_or_else(|| Error::msg("Bencode integer must ends with e"))
            .and_then(|(num, rest)| {
                num.parse::<i64>()
                    .map_err(Error::from)
                    .map(|num| Value::Number(Number::from(num)))
                    .map(|num| (rest, num))
            }),
        'l' => {
            let mut values = vec![];
            let mut rest = &encoded_value[1..];
            while !rest.starts_with('e') {
                let (encoded_value, value) = decode_bencoded_value(rest)?;
                rest = encoded_value;
                values.push(value);
            }
            Ok((&rest[1..], Value::Array(values)))
        }
        'd' => {
            let mut map = Map::new();
            let mut rest = &encoded_value[1..];
            while !rest.starts_with('e') {
                let (encoded_value, key) = decode_bencoded_value(rest)?;
                rest = encoded_value;
                if let Value::String(key) = key {
                    let (encoded_value, value) = decode_bencoded_value(rest)?;
                    map.insert(key, value);
                    rest = encoded_value;
                } else {
                    return Err(Error::msg(format!(
                        "Bencode dictinary key must be a string. Value: {encoded_value}"
                    )));
                }
            }
            Ok((&rest[1..], Value::Object(map)))
        }
        _ if first_char.is_ascii_digit() => encoded_value
            .split_once(':')
            .and_then(|(length, rest)| {
                let length = length.parse::<usize>().ok()?;
                Some((
                    &rest[length..],
                    serde_json::Value::String(rest[0..length].to_string()),
                ))
            })
            .ok_or_else(|| Error::msg(format!("Invalid encoded string {encoded_value}"))),
        _ => Err(Error::msg(format!(
            "Unexpected bencode value {encoded_value}"
        ))),
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
        let (_, decoded_value) = decode_bencoded_value(encoded_value)?;
        println!("{}", decoded_value);
    } else {
        println!("unknown command: {}", args[1])
    }
    Ok(())
}
