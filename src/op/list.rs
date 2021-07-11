use console::Term;
use once_cell::sync::Lazy;
use regex::Regex;
use rumqttc::QoS;
use single::Single;
use std::{collections::HashMap, io, io::Write};

use crate::{data::decode, net::mqtt, op::TopicBundle};

static INFO_TOPIC_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"hoek/iot/([a-zA-Z0-9-_]+)/_info/([a-zA-Z0-9-_]+)").unwrap());

struct DeviceDisplayInfo {
    status_fmt: Option<String>,
    id_fmt: Option<String>,
}

impl DeviceDisplayInfo {
    fn new() -> Self {
        Self {
            status_fmt: None,
            id_fmt: None,
        }
    }

    fn integrate_status_fmt(&mut self, status_fmt: String) {
        self.status_fmt = Some(status_fmt);
    }

    fn integrate_id_fmt(&mut self, id_fmt: String) {
        self.id_fmt = Some(id_fmt);
    }
}

const FIRST_INDENT: &str = "  * ";
const SECOND_INDENT: &str = "    ";
const NL_INDENT: &str = "\n    ";

fn print_device_display_info(devs: &HashMap<String, DeviceDisplayInfo>) -> usize {
    let mut devs_sorted: Vec<(&String, &DeviceDisplayInfo)> = devs.iter().collect();
    devs_sorted.sort_by_key(|(s, _)| *s);

    let mut lines_printed = 1;

    println!();
    for (device_name, info) in devs_sorted {
        print!("{}{}: ", FIRST_INDENT, device_name);

        if let Some(status_fmt) = &info.status_fmt {
            println!("{}", status_fmt.replace("\n", NL_INDENT));
            lines_printed += 1 + status_fmt.chars().filter(|c| *c == '\n').count();
        } else {
            println!();
            lines_printed += 1;
        }

        if let Some(id_fmt) = &info.id_fmt {
            println!("{}{}", SECOND_INDENT, id_fmt.replace("\n", NL_INDENT));
            lines_printed += 1 + id_fmt.chars().filter(|c| *c == '\n').count();
        }

        println!();
        lines_printed += 1;
    }

    lines_printed
}

pub fn perform() -> ! {
    let topics = TopicBundle::new("+");

    let term = Term::stdout();

    println!("Connecting to broker...");

    let (mut client, rx) = mqtt::connect();

    term.clear_last_lines(1).unwrap();
    println!("Listing discovered devices...");
    decode::print_parts_legend();

    client
        .subscribe(topics.info_status, QoS::ExactlyOnce)
        .unwrap();

    client.subscribe(topics.info_id, QoS::ExactlyOnce).unwrap();

    let mut devs: HashMap<String, DeviceDisplayInfo> = HashMap::new();
    let mut last_line_count = 0;

    loop {
        let msg = rx.recv().unwrap();

        let captures = INFO_TOPIC_REGEX
            .captures_iter(&msg.topic)
            .into_iter()
            .single()
            .unwrap();

        assert!(captures.len() == 3);

        let device_name = captures.get(1).unwrap().as_str();
        let suffix = captures.get(2).unwrap().as_str();

        let dev = devs
            .entry(device_name.to_owned())
            .or_insert_with(DeviceDisplayInfo::new);

        match suffix {
            "status" => dev.integrate_status_fmt(decode::decode_status_message(
                &serde_json::from_str(&msg.payload).expect("JSON parse error"),
            )),
            "id" => dev.integrate_id_fmt(
                decode::decode_id_message(
                    serde_json::from_str(&msg.payload).expect("JSON parse error"),
                )
                .ota_info
                .fmt,
            ),
            _ => panic!("unknown suffix {}", suffix),
        };

        term.clear_last_lines(last_line_count).unwrap();
        last_line_count = print_device_display_info(&devs);

        print!("<Press Ctrl-C to stop live update>");
        io::stdout().flush().unwrap();
    }
}
