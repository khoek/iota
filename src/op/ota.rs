use console::style;
use console::Term;
use rumqttc::QoS;
use std::fmt::Display;
use std::io::{self, Read};
use std::sync::mpsc::Receiver;

use crate::{
    data::{decode, model},
    net::mqtt,
    op,
};

impl Display for model::OtaMessage {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        use model::OtaMessage;
        match self {
            OtaMessage::Start => write!(out, "{}", style("Start").yellow()),
            OtaMessage::InProgress { rx_kb } => {
                write!(out, "{}, read {} kB", style("In Progress").white(), rx_kb)
            }
            OtaMessage::Done => write!(out, "{}", style("Done").green()),
            OtaMessage::Fail => write!(out, "{}", style("Fail").red()),
        }
    }
}

impl model::OtaMessage {
    fn is_terminal(&self) -> bool {
        use model::OtaMessage;
        match self {
            OtaMessage::Start => false,
            OtaMessage::InProgress { .. } => false,
            OtaMessage::Done => true,
            OtaMessage::Fail => true,
        }
    }
}

pub struct Operation<'a> {
    pub url: &'a str,
    pub ca_cert: &'a str,
}

impl op::Operation for Operation<'_> {
    fn perform(
        &self,
        topics: &super::TopicBundle,
        (client, rx): (&mut rumqttc::Client, &Receiver<mqtt::MqttPacket>),
        id: &decode::DecodedIdMessage,
    ) -> op::ExitDisposition {
        let term = Term::stdout();

        match (id.ota_info.running_on_part, id.ota_info.running_ota_state) {
            (_, model::OtaState::PendingVerify) => {
                println!(
                    "{}: Device reports OTA update already pending!",
                    op::PrettyHeader::Failed
                );
                println!("Use `iota validate` or `iota rollback` to clear this status (use Ctrl-C to abort the current operation).");
                println!();
                println!("<Press any key to restart device and retry>");
                io::stdin().read_exact(&mut [0]).unwrap();

                client
                    .publish(&topics.cmd_restart, QoS::ExactlyOnce, false, "")
                    .unwrap();

                println!("Restart command sent...");

                return op::ExitDisposition::Retry;
            }
            (decode::RunningOnPart::Factory, model::OtaState::NotPresent)
            | (decode::RunningOnPart::Ota { .. }, model::OtaState::Valid)
            | (decode::RunningOnPart::Ota { .. }, model::OtaState::Undefined) => {}
            state => {
                panic!(
                    "running on partition with unexpected OTA state: {:?}",
                    state
                );
            }
        };

        println!("Sending OTA command...");

        client
            .publish(
                &topics.cmd_ota,
                QoS::ExactlyOnce,
                false,
                serde_json::to_string(&model::OtaCommand::Update {
                    url: self.url,
                    ca_cert: self.ca_cert,
                })
                .expect("Could not build JSON")
                .as_bytes(),
            )
            .unwrap();

        term.clear_last_lines(1).unwrap();
        println!("OTA command sent, listening for updates...");

        loop {
            let msg = rx.recv().unwrap();

            if msg.topic == topics.info_ota {
                let ota_state: model::OtaMessage =
                    serde_json::from_str(&msg.payload).expect("JSON parse error");

                println!("  ota: {}", ota_state);

                match ota_state {
                    model::OtaMessage::Done => {
                        println!("OTA upload complete, restarting device...");
                        break;
                    }
                    state if state.is_terminal() => {
                        println!("OTA upload failed, aborting");
                        return op::ExitDisposition::Abort;
                    }
                    _ => {}
                };
            }

            if msg.topic == topics.info_error {
                println!("{} ({})", style("Log Error").red(), msg.payload);
            }
        }

        client
            .publish(&topics.cmd_restart, QoS::ExactlyOnce, false, "")
            .unwrap();

        op::ExitDisposition::Ok
    }

    fn get_wait_strategy(&self) -> Option<op::PostOperationWaitStrategy> {
        Some(op::PostOperationWaitStrategy::IdMessage)
    }

    fn exit_ok_is_finished_waiting(
        &self,
        original_id: &decode::DecodedIdMessage,
        current_id: &decode::DecodedIdMessage,
    ) -> bool {
        match current_id.ota_info.running_on_part {
            decode::RunningOnPart::Factory => {
                panic!("unexpectedly running on factory partition after OTA!")
            }
            decode::RunningOnPart::Ota { .. } => {
                if current_id.ota_info.running_addr == original_id.ota_info.running_addr {
                    return false;
                }

                if current_id.ota_info.running_addr == original_id.ota_info.next_update_addr {
                    return true;
                }

                panic!(
                    "unexpectedly running on partition with address 0x{:x}, instead of 0x{:x}",
                    current_id.ota_info.running_addr, original_id.ota_info.next_update_addr
                );
            }
        }
    }

    fn exit_retry_is_finished_waiting(
        &self,
        _: &decode::DecodedIdMessage,
        current_id: &decode::DecodedIdMessage,
    ) -> bool {
        current_id.ota_info.running_ota_state != model::OtaState::PendingVerify
    }

    fn print_completed_message(&self) {
        println!(
            "{}: Device restarted, OTA successful!",
            op::PrettyHeader::Success
        );
        println!("Use `iota validate <device>` to mark the update as permanent.");
        println!("Use `iota rollback <device>` to rollback to the previous version.");
    }
}
