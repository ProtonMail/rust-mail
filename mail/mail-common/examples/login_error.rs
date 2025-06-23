#![allow(clippy::print_stdout)]
use clap::Parser;
use proton_account_api::login::LoginFlow;
use proton_account_api::shared::challenge::ChallengeInfo;
use proton_core_api::session::Session;

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
    let mut login_flow = LoginFlow::new(session, ChallengeInfo::default());
    let result = login_flow
        .login_with_credentials(username, password, None)
        .await;

    match result {
        Ok(_) => {
            println!("Login success!")
        }
        Err(error) => {
            println!("Inner error: {error:?}");
        }
    }
}
