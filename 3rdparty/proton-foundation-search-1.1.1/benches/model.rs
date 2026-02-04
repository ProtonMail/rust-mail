#![allow(unused)]

#[derive(Clone, Debug, serde::Deserialize)]
pub struct Person {
    pub name: String,
    pub email: String,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct Mail {
    pub id: String,
    pub subject: String,
    pub body: String,
    #[serde(alias = "sender")]
    pub from: Person,
    #[serde(default)]
    pub to: Vec<Person>,
    #[serde(default)]
    pub cc: Vec<Person>,
    #[serde(default)]
    pub bcc: Vec<Person>,
    pub time: u64,
}
