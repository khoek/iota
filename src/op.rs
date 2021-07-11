pub mod list;
pub mod mark;
pub mod ota;
pub mod restart;

use console::{style, Term};
use rumqttc::QoS;
use std::fmt;
use std::sync::mpsc::Receiver;

use crate::data::{
    decode::{self, decode_id_message},
    model,
};
use crate::net::mqtt::{self, MqttPacket};

pub(crate) struct TopicBundle {
    info_ota: String,
    info_error: String,
    info_status: String,
    info_id: String,

    cmd_ota: String,
    cmd_restart: String,
}

impl TopicBundle {
    fn new(device_name: &str) -> Self {
        TopicBundle {
            info_ota: r"hoek/iot/".to_owned() + device_name + "/_info/ota",
            info_error: r"hoek/iot/".to_owned() + device_name + "/_info/error",
            info_status: r"hoek/iot/".to_owned() + device_name + "/_info/status",
            info_id: r"hoek/iot/".to_owned() + device_name + "/_info/id",

            cmd_ota: r"hoek/iot/".to_owned() + device_name + "/_cmd/ota",
            cmd_restart: r"hoek/iot/".to_owned() + device_name + "/_cmd/restart",
        }
    }
}

pub(crate) enum PrettyHeader {
    Success,
    Failed,
}

impl fmt::Display for PrettyHeader {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrettyHeader::Success => write!(fmt, "{}", style("SUCCESS").on_green()),
            PrettyHeader::Failed => write!(fmt, "{}", style("FAILED").on_red()),
        }
    }
}

pub enum ExitDisposition {
    Ok,
    Retry,
    Abort,
}

pub(crate) trait Operation {
    fn body(
        &self,
        topics: &TopicBundle,
        mqtt: (&mut rumqttc::Client, &Receiver<MqttPacket>),
        id: &decode::DecodedIdMessage,
    ) -> ExitDisposition;

    fn should_wait_for_power_cycle(&self) -> bool;

    fn exit_ok_is_finished_waiting(
        &self,
        _original_id: &decode::DecodedIdMessage,
        _current_id: &decode::DecodedIdMessage,
    ) -> bool {
        true
    }

    fn exit_retry_is_finished_waiting(
        &self,
        _original_id: &decode::DecodedIdMessage,
        _current_id: &decode::DecodedIdMessage,
    ) -> bool {
        unreachable!()
    }
}

fn perform_op_once<Op: Operation>(op: Op, device_name: &str) -> ExitDisposition {
    let term = Term::stdout();

    let topics = TopicBundle::new(device_name);

    println!("Connecting to broker...");

    let (mut client, rx) = mqtt::connect();

    client
        .subscribe(&topics.info_ota, QoS::ExactlyOnce)
        .unwrap();

    client
        .subscribe(&topics.info_error, QoS::ExactlyOnce)
        .unwrap();

    client.subscribe(&topics.info_id, QoS::ExactlyOnce).unwrap();
    client
        .subscribe(&topics.info_status, QoS::ExactlyOnce)
        .unwrap();

    term.clear_last_lines(1).unwrap();
    println!(
        "Waiting for status message from device '{}'...",
        device_name
    );

    let original_id_raw = match mqtt_wait_for_id_message(
        Some(&topics.info_status),
        &topics.info_id,
        &topics.info_error,
        &rx,
    ) {
        None => {
            println!("{}: Device is down!", PrettyHeader::Failed);
            return ExitDisposition::Abort;
        }
        Some(original_id_raw) => original_id_raw,
    };

    let original_id =
        decode_id_message(serde_json::from_str(&original_id_raw).expect("JSON parse error"));

    term.clear_last_lines(1).unwrap();
    decode::print_parts_legend();
    println!();
    println!("{}", original_id.ota_info.fmt);
    println!();

    let ed = op.body(&topics, (&mut client, &rx), &original_id);

    if let ExitDisposition::Abort = ed {
        return ExitDisposition::Abort;
    }

    if op.should_wait_for_power_cycle() {
        println!("Waiting for device 'Down' message...");

        mqtt_wait_for_status_message(&topics.info_status, model::DeviceState::Down, &rx);

        term.clear_last_lines(1).unwrap();
        println!("Waiting for device 'Up' message...");

        mqtt_wait_for_status_message(&topics.info_status, model::DeviceState::Up, &rx);

        println!("Device reconnected!");
    }

    match ed {
        ExitDisposition::Ok => {
            println!();
            mqtt_wait_for_id_condition(&topics, &rx, &original_id, |o_id, c_id| {
                op.exit_ok_is_finished_waiting(o_id, c_id)
            })
        }
        ExitDisposition::Retry => {
            println!();
            mqtt_wait_for_id_condition(&topics, &rx, &original_id, |o_id, c_id| {
                op.exit_retry_is_finished_waiting(o_id, c_id)
            })
        }
        ExitDisposition::Abort => unreachable!(),
    }

    ed
}

// Returns `true` if the operation completed, and `false` if it should be
// retried. (If an un-retriable error occurs, the program will exit.)
pub(crate) fn perform_op<Op: Operation>(op: Op, device_name: &str) -> bool {
    loop {
        match perform_op_once(op, device_name) {
            ExitDisposition::Retry => {
                println!("Retrying operation...");
                println!();

                return false;
            }
            ExitDisposition::Ok => {
                return true;
            }
            ExitDisposition::Abort => {
                std::process::exit(-1);
            }
        }
    }
}

fn mqtt_wait_for_id_condition<
    FCond: Fn(&decode::DecodedIdMessage, &decode::DecodedIdMessage) -> bool,
>(
    topics: &TopicBundle,
    rx: &Receiver<MqttPacket>,
    original_id: &decode::DecodedIdMessage,
    condition: FCond,
) {
    let term = Term::stdout();

    println!("Waiting for response(s)...");

    loop {
        let raw_id =
            mqtt_wait_for_id_message(None, &topics.info_id, &topics.info_error, rx).unwrap();
        let current_id =
            decode_id_message(serde_json::from_str(&raw_id).expect("JSON parse error"));

        term.clear_last_lines(1).unwrap();
        println!("{}", current_id.ota_info.fmt);
        println!();

        if condition(original_id, &current_id) {
            break;
        }

        println!("Waiting for newer response(s)...");
    }
}

fn mqtt_wait_for_status_message(
    topic_info_status: &str,
    target_state: model::DeviceState,
    rx: &Receiver<mqtt::MqttPacket>,
) {
    loop {
        let msg = rx.recv().unwrap();

        if msg.topic == topic_info_status {
            let status: model::StatusMessage =
                serde_json::from_str(&msg.payload).expect("JSON parse error");

            if status.state == target_state {
                return;
            }
        }
    }
}

// If `topic_info_status` is present, we don't return until we have seen
// an "up" message from the device, and we abort if we ever recieve a
// "down" status message (returning `None`).
fn mqtt_wait_for_id_message(
    topic_info_status: Option<&str>,
    topic_info_id: &str,
    topic_info_error: &str,
    rx: &Receiver<mqtt::MqttPacket>,
) -> Option<String> {
    let mut up_state_seen = false;
    let mut last_raw_id: Option<String> = None;

    loop {
        let msg = rx.recv().unwrap();

        if msg.topic == topic_info_error {
            println!("{} ({})", style("Log Error").red(), msg.payload);
            // Prevent the message being eaten when the previous line is cleared.
            println!();
        }

        if topic_info_status.map_or(false, |topic| msg.topic == topic) {
            let status: model::StatusMessage =
                serde_json::from_str(&msg.payload).expect("JSON parse error");

            match status.state {
                model::DeviceState::Up => {
                    up_state_seen = true;
                    if last_raw_id.is_some() {
                        break;
                    }
                }
                model::DeviceState::Down => return None,
            }
        }

        if msg.topic == topic_info_id {
            last_raw_id = Some(msg.payload);

            if up_state_seen || topic_info_status.is_none() {
                break;
            }
        }
    }

    last_raw_id
}
