#![allow(clippy::print_stdout)]
use clap::Parser;
use proton_core_api::login::LoginFlow;
use proton_core_api::services::proton::muon::client::flow::LoginExtraInfo;
use proton_core_api::session::Session;
use proton_mail_common::errors::ProtonMailError;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    username: String,
    #[arg(short, long)]
    password: String,
}

#[tokio::main]
async fn main() {
    let Args { username, password } = Args::parse();

    let session = Session::new().await.unwrap();
    let mut login_flow = LoginFlow::new(session);
    let result = login_flow
        .login(username, password, LoginExtraInfo::default())
        .await;

    match result {
        Ok(_) => {
            println!("Login success!")
        }
        Err(error) => {
            println!("Inner error: {error:?}");
            let error = ProtonMailError::from(error);
            println!("User error: {error:?}");
        }
    }
}
