use console::{style, Color};
use single::Single;
use std::fmt::{Display, Write};

use super::model;

impl Display for model::DeviceState {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            model::DeviceState::Up => write!(out, "{}", style("Up").green()),
            model::DeviceState::Down => write!(out, "{}", style("Down").red()),
        }
    }
}

pub fn decode_status_message(status: &model::StatusMessage) -> String {
    let mut fmt = String::new();
    write!(fmt, "{}", status.state).unwrap();
    fmt
}

pub struct DecodedIdMessage<'a> {
    pub msg: model::IdMessage<'a>,
    pub ota_info: RuntimeOtaInfo,
}

pub struct RuntimeOtaInfo {
    pub running_addr: usize,
    pub running_on_part: RunningOnPart,
    pub running_ota_state: model::OtaState,

    pub next_update_addr: usize,

    pub fmt: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RunningOnPart {
    Factory,
    Ota { id: usize },
}

pub fn decode_id_message(id: model::IdMessage) -> DecodedIdMessage {
    let mut fmt = String::new();

    {
        let model::AppDesc {
            project_name,
            version,
            secure_version,
            date,
            time,
        } = id.software.app_desc;

        writeln!(
            fmt,
            "Reported firmware: {}/{} ({}), built {} {}",
            project_name, version, secure_version, date, time
        )
        .unwrap();
    }

    let model::Partitions {
        list,
        running: running_addr,
        boot: boot_addr,
        next_update: next_update_addr,
        last_invalid: last_invalid_addr,
        ..
    } = &id.software.partitions;

    let mut list = list.iter().collect::<Vec<&model::Partition>>();

    let running_addr = running_addr.expect("running partition address not reported!");

    let (running_type, running_ota_state) = list
        .iter()
        .filter(|p| p.address == running_addr)
        .map(|p| (p.part_type, p.ota_state))
        .single()
        .expect("multiple running partition");

    let running_on_part = match running_type {
        model::PartitionType::App(model::PartitionAppSubtype::Factory) => RunningOnPart::Factory,
        model::PartitionType::App(model::PartitionAppSubtype::Ota { id }) => {
            RunningOnPart::Ota { id }
        }
        model::PartitionType::App(subtype) => {
            panic!("running on unknown App partition subtype: {:?}", subtype);
        }
        part_type => {
            panic!("running on unknown partition type: {:?}", part_type);
        }
    };

    list.sort_by_key(|p| p.address);

    write!(fmt, "       Partitions: ").unwrap();
    for part in list.iter() {
        let sym = match part.part_type {
            model::PartitionType::Data(_) => "~",
            model::PartitionType::App(model::PartitionAppSubtype::Factory) => "F",
            model::PartitionType::App(model::PartitionAppSubtype::Test) => "T",
            model::PartitionType::App(model::PartitionAppSubtype::Ota { .. }) => "O",
        };

        let style = style(sym).black();
        let style = match part.ota_state {
            model::OtaState::Aborted => style.bg(Color::Red),
            model::OtaState::Invalid => style.bg(Color::Red),
            model::OtaState::New => style.bg(Color::Yellow),
            model::OtaState::NotPresent => style.bg(Color::White),
            model::OtaState::PendingVerify => style.bg(Color::Yellow),
            model::OtaState::Undefined => style.bg(Color::Cyan),
            model::OtaState::Valid => style.bg(Color::Green),
        };
        write!(fmt, "{}", style).unwrap();
    }

    write!(fmt, ", running on ").unwrap();
    match running_on_part {
        RunningOnPart::Factory => {
            write!(fmt, "factory partition (0x{:x})", running_addr)
        }
        RunningOnPart::Ota { id } => {
            write!(fmt, "ota {} partition (0x{:x})", id, running_addr)
        }
    }
    .unwrap();
    writeln!(fmt).unwrap();

    write!(fmt, "                   ").unwrap();
    for part in list {
        let sym = if running_addr == part.address {
            "R"
        } else if boot_addr.map_or(false, |addr| addr == part.address) {
            "B"
        } else if next_update_addr.map_or(false, |addr| addr == part.address) {
            "U"
        } else if last_invalid_addr.map_or(false, |addr| addr == part.address) {
            "I"
        } else {
            " "
        };
        write!(fmt, "{}", sym).unwrap();
    }

    let next_update_addr = match *next_update_addr {
        None => panic!("device reports no free OTA partition for upload"),
        Some(addr) => addr,
    };

    DecodedIdMessage {
        msg: id,
        ota_info: RuntimeOtaInfo {
            running_addr,
            running_on_part,
            running_ota_state,

            next_update_addr,

            fmt,
        },
    }
}

pub fn print_parts_legend() {
    println!("Legend (parts): (~)Data (F)App:Factory (T)App:Test (O)App:Ota");
    println!("       (flags): (R)Running (B)Boot (U)NextUpdate (I)LastInvalid");
    println!(
        "       (state): {} {} {}/{} {}/{} {}",
        style("NotPresent").black().bg(Color::White),
        style("Valid").black().bg(Color::Green),
        style("New").black().bg(Color::Yellow),
        style("PendingVerify").black().bg(Color::Yellow),
        style("Aborted").black().bg(Color::Red),
        style("Invalid").black().bg(Color::Red),
        style("Undefined").black().bg(Color::Cyan),
    );
}
