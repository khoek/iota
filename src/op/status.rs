use std::sync::mpsc::Receiver;

use crate::{data::decode, net::mqtt, op};

pub struct Operation {}

impl op::Operation for Operation {
    fn perform(
        &self,
        _: &super::TopicBundle,
        _: (&mut rumqttc::Client, &Receiver<mqtt::MqttPacket>),
        _: &decode::DecodedIdMessage,
    ) -> op::ExitDisposition {
        op::ExitDisposition::Ok
    }

    fn get_wait_strategy(&self) -> Option<op::PostOperationWaitStrategy> {
        None
    }

    fn exit_ok_is_finished_waiting(
        &self,
        _original_id: &super::decode::DecodedIdMessage,
        _current_id: &super::decode::DecodedIdMessage,
    ) -> bool {
        true
    }

    fn print_completed_message(&self) {
        println!("{}: Device status found!", op::PrettyHeader::Success);
    }
}
