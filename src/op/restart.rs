use rumqttc::QoS;
use std::sync::mpsc::Receiver;

use crate::{data::decode, net::mqtt, op};

pub struct Operation {}

impl op::Operation for Operation {
    fn body(
        &self,
        topics: &super::TopicBundle,
        (client, _): (&mut rumqttc::Client, &Receiver<mqtt::MqttPacket>),
        _: &decode::DecodedIdMessage,
    ) -> op::ExitDisposition {
        println!("Sending restart command...");

        client
            .publish(&topics.cmd_restart, QoS::ExactlyOnce, false, "")
            .unwrap();

        op::ExitDisposition::Ok
    }

    fn should_wait_for_power_cycle(&self) -> bool {
        true
    }

    fn exit_ok_is_finished_waiting(
        &self,
        _original_id: &super::decode::DecodedIdMessage,
        _current_id: &super::decode::DecodedIdMessage,
    ) -> bool {
        println!("{}: Restart completed!", op::PrettyHeader::Success);

        true
    }
}
