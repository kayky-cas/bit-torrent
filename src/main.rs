use std::{
    env,
    io::{BufRead, BufReader},
};

use anyhow::Context;
use serde_json::Map;

// Available if you need it!
// use serde_bencode

fn decode_bencoded_value(reader: &mut dyn BufRead) -> anyhow::Result<serde_json::Value> {
    // If encoded_value starts with a digit, it's a number

    let mut header_buff = [0; 1];
    let _ = reader.read_exact(&mut header_buff);

    Ok(match header_buff[0] {
        ch if ch.is_ascii_digit() => {
            let mut buf = Vec::new();
            let _ = reader
                .read_until(b':', &mut buf)
                .context("not a valid string")?;

            buf.insert(0, ch);

            let size: usize = std::str::from_utf8(&buf[..buf.len() - 1])
                .context("the size is not a valid UTF-8")?
                .parse()
                .context("the size is not a number")?;

            buf.resize(size, 0);

            let buf = &mut buf[..size];

            reader
                .read_exact(buf)
                .context("not possible to read the string")?;

            let text = std::str::from_utf8(buf).context("the size is not a valid UTF-8")?;
            serde_json::Value::String(text.to_string())
        }
        b'i' => {
            let mut buf = Vec::new();

            let _ = reader
                .read_until(b'e', &mut buf)
                .context("not a valid integer")?;

            let num: i64 = std::str::from_utf8(&buf[..buf.len() - 1])
                .context("the integer is not a valid UTF-8")?
                .parse()
                .context("it's not an integer")?;

            serde_json::Value::Number(num.into())
        }
        b'l' => {
            let mut list: Vec<serde_json::Value> = Vec::new();

            while let Ok(value) = decode_bencoded_value(reader) {
                list.push(value);
            }

            list.into()
        }
        b'd' => {
            let mut list: Map<String, serde_json::Value> = Map::new();

            while let Ok(serde_json::Value::String(value)) = decode_bencoded_value(reader) {
                list.insert(value, decode_bencoded_value(reader)?);
            }

            serde_json::Value::Object(list)
        }
        _ => {
            anyhow::bail!("This is not a valid bencode value")
        }
    })
}

// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];

    if command == "decode" {
        let encoded_value = &args[2];
        let mut reader = BufReader::new(encoded_value.as_bytes());
        let decoded_value = decode_bencoded_value(&mut reader)
            .with_context(|| format!("was not possible to decode: {}", encoded_value))?;
        println!("{}", decoded_value);
    } else {
        println!("unknown command: {}", args[1])
    }

    Ok(())
}
