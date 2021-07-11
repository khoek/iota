use rumqttc::QoS;
use single::Single;
use std::fmt;
use std::sync::mpsc::Receiver;

use crate::{
    data::{decode, model},
    net::mqtt,
    op::{self, PrettyHeader},
};

#[derive(Debug, Clone, Copy)]
pub enum Mark {
    Validate,
    Rollback,
}

impl fmt::Display for Mark {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Mark::Validate => write!(fmt, "validate"),
            Mark::Rollback => write!(fmt, "rollback"),
        }
    }
}

impl Mark {
    fn get_ota_command(&self) -> model::OtaCommand {
        match self {
            Mark::Validate => model::OtaCommand::Validate,
            Mark::Rollback => model::OtaCommand::Rollback,
        }
    }

    fn is_acceptable_initial_ota_state(&self, state: model::OtaState) -> bool {
        matches!(
            (self, state),
            (Mark::Validate, model::OtaState::PendingVerify)
                | (Mark::Rollback, model::OtaState::PendingVerify)
                | (Mark::Rollback, model::OtaState::Valid)
        )
    }

    fn is_command_completed(
        &self,
        original_id: &decode::DecodedIdMessage,
        current_id: &decode::DecodedIdMessage,
    ) -> bool {
        match self {
            Mark::Validate => {
                if current_id.ota_info.running_addr != original_id.ota_info.running_addr {
                    panic!(
                        "unexpectedly running on partition with address 0x{:x}, instead of 0x{:x}",
                        current_id.ota_info.running_addr, original_id.ota_info.running_addr
                    );
                }

                match (
                    current_id.ota_info.running_on_part,
                    current_id.ota_info.running_ota_state,
                ) {
                    (decode::RunningOnPart::Factory, _) => {
                        panic!("unexpectedly running on factory partition after validate!")
                    }
                    (decode::RunningOnPart::Ota { .. }, model::OtaState::PendingVerify) => false,
                    (decode::RunningOnPart::Ota { .. }, model::OtaState::Valid) => true,
                    (decode::RunningOnPart::Ota { .. }, state) => {
                        panic!(
                            "unexpectedly running on partition with ota state {:?}",
                            state
                        );
                    }
                }
            }
            Mark::Rollback => {
                let original_part_current_state = current_id
                    .msg
                    .software
                    .partitions
                    .list
                    .iter()
                    .filter(|p| p.address == original_id.ota_info.running_addr)
                    .single()
                    .unwrap();

                let running_part_has_changed =
                    current_id.ota_info.running_addr != original_id.ota_info.running_addr;

                match original_part_current_state.ota_state {
                    model::OtaState::PendingVerify => {
                        if running_part_has_changed {
                            panic!(
                                "unexpectedly running on partition with address 0x{:x}, instead of 0x{:x}, with original partition still in state `PendingVerify`",
                                current_id.ota_info.running_addr, original_id.ota_info.running_addr);
                        }

                        false
                    }
                    model::OtaState::Invalid => {
                        if !running_part_has_changed {
                            panic!(
                                "unexpectedly running on partition original partition after state has changed to `Invalid`");
                        }

                        true
                    }
                    state => {
                        panic!(
                            "unexpectedly running on partition with ota state {:?}",
                            state
                        );
                    }
                }
            }
        }
    }
}

pub struct Operation {
    pub mark: Mark,
}

impl op::Operation for Operation {
    fn perform(
        &self,
        topics: &super::TopicBundle,
        (client, _): (&mut rumqttc::Client, &Receiver<mqtt::MqttPacket>),
        id: &decode::DecodedIdMessage,
    ) -> op::ExitDisposition {
        match id.ota_info.running_on_part {
            decode::RunningOnPart::Ota { .. } => {}
            decode::RunningOnPart::Factory => {
                println!(
                    "{}: Cannot modify OTA state of a factory partition!",
                    op::PrettyHeader::Failed,
                );

                return op::ExitDisposition::Abort;
            }
        };

        if !self
            .mark
            .is_acceptable_initial_ota_state(id.ota_info.running_ota_state)
        {
            println!(
                "{}: OTA state of running parition ({:?}) is not acceptable for a {} operation!",
                op::PrettyHeader::Failed,
                id.ota_info.running_ota_state,
                self.mark
            );

            return op::ExitDisposition::Abort;
        }

        if !id.msg.software.partitions.is_rollback_possible {
            println!(
                "{}: Device reports that rollback is not possible!",
                op::PrettyHeader::Failed,
            );

            return op::ExitDisposition::Abort;
        }

        println!("Sending {} command...", self.mark);

        // Note that the rollback command actually causes a device restart when it successfully completes.
        client
            .publish(
                &topics.cmd_ota,
                QoS::ExactlyOnce,
                false,
                serde_json::to_string(&self.mark.get_ota_command())
                    .expect("Could not build JSON")
                    .as_bytes(),
            )
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
        self.mark.is_command_completed(original_id, current_id)
    }

    fn print_completed_message(&self) {
        println!(
            "{}: Operation {} (of running partition) successful!",
            PrettyHeader::Success,
            self.mark
        );
    }
}
