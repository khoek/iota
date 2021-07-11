pub mod data;
pub mod net;
pub mod op;

use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub enum CommandRoot {
    List(SubcommandList),
    Ota(SubcommandOta),
    Restart(SubcommandRestart),
    Validate(SubcommandValidate),
    Rollback(SubcommandRollback),
}

#[derive(StructOpt, Debug)]
#[structopt(name = "list")]
pub struct SubcommandList {}

#[derive(StructOpt, Debug)]
#[structopt(name = "ota")]
pub struct SubcommandOta {
    device: String,
    file: PathBuf,
}

#[derive(StructOpt, Debug)]
#[structopt(name = "restart")]
pub struct SubcommandRestart {
    device: String,
}

#[derive(StructOpt, Debug)]
#[structopt(name = "validate")]
pub struct SubcommandValidate {
    device: String,
}

#[derive(StructOpt, Debug)]
#[structopt(name = "rollback")]
pub struct SubcommandRollback {
    device: String,
}

fn command_list(_: SubcommandList) {
    op::list::perform();
}

fn command_ota(cmd: SubcommandOta) {
    loop {
        let url = net::https::upload_tmp_file(cmd.file.clone());
        let ca_cert = net::https::download_root_ca_cert_pem(&url);

        println!("-------------------");
        println!("Starting OTA Update");
        println!("-------------------");

        if op::perform_op(
            op::ota::Operation {
                url: &url,
                ca_cert: &ca_cert,
            },
            &cmd.device,
        ) {
            break;
        }
    }
}

fn command_restart(cmd: SubcommandRestart) {
    op::perform_op(op::restart::Operation {}, &cmd.device);
}

fn command_validate(cmd: SubcommandValidate) {
    op::perform_op(
        op::mark::Operation {
            mark: op::mark::Mark::Validate,
        },
        &cmd.device,
    );
}

fn command_rollback(cmd: SubcommandRollback) {
    op::perform_op(
        op::mark::Operation {
            mark: op::mark::Mark::Rollback,
        },
        &cmd.device,
    );
}

fn main() {
    match CommandRoot::from_args() {
        CommandRoot::List(cmd) => command_list(cmd),
        CommandRoot::Ota(cmd) => command_ota(cmd),
        CommandRoot::Restart(cmd) => command_restart(cmd),
        CommandRoot::Validate(cmd) => command_validate(cmd),
        CommandRoot::Rollback(cmd) => command_rollback(cmd),
    }
}
