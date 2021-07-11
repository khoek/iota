use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceState {
    Up,
    Down,
}

#[derive(Debug, Deserialize)]
pub struct StatusMessage {
    pub state: DeviceState,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum OtaMessage {
    Start,
    InProgress { rx_kb: usize },
    Done,
    Fail,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum OtaCommand<'a> {
    Update { url: &'a str, ca_cert: &'a str },
    Validate,
    Rollback,
}

#[derive(Debug, Deserialize)]
pub struct AppDesc<'a> {
    pub project_name: &'a str,
    pub version: &'a str,
    pub secure_version: usize,
    pub date: &'a str,
    pub time: &'a str,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case", tag = "name", content = "subtype")]
pub enum PartitionType {
    App(PartitionAppSubtype),
    Data(PartitionDataSubtype),
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case", tag = "name")]
pub enum PartitionAppSubtype {
    Factory,
    Test,
    Ota { id: usize },
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case", tag = "name")]
pub enum PartitionDataSubtype {
    Ota,
    Phy,
    Nvs,
    CoreDump,
    NvsKeys,
    EfuseEm,
    Esphttpd,
    Fat,
    Spiffs,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OtaState {
    NotPresent,
    New,
    PendingVerify,
    Valid,
    Invalid,
    Aborted,
    Undefined,
}

#[derive(Debug, Deserialize)]
pub struct Partition<'a> {
    pub flash_chip_id: usize,
    #[serde(rename = "type")]
    pub part_type: PartitionType,
    pub address: usize,
    pub size: usize,
    pub label: &'a str,
    pub encrypted: bool,
    pub ota_state: OtaState,
}

#[derive(Debug, Deserialize)]
pub struct Partitions<'a> {
    pub boot: Option<usize>,
    pub running: Option<usize>,
    pub last_invalid: Option<usize>,
    pub next_update: Option<usize>,
    pub is_rollback_possible: bool,

    #[serde(borrow)]
    pub list: Vec<Partition<'a>>,
}

#[derive(Debug, Deserialize)]
pub struct Software<'a> {
    #[serde(borrow)]
    pub app_desc: AppDesc<'a>,
    pub partitions: Partitions<'a>,
}

#[derive(Debug, Deserialize)]
pub struct IdMessage<'a> {
    #[serde(borrow)]
    pub software: Software<'a>,
}
