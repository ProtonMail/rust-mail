use proton_core_api::services::proton::Label as ApiLabel;
use proton_core_api::services::proton::LabelId;
use proton_core_api::services::proton::LabelType;
use proton_mail_common::Sidebar;
use proton_mail_common::datatypes::labels::custom_folder::CustomFolder;
use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::test_context::MailTestContext;
use std::iter::zip;
use test_case::test_case;
use velcro::hash_map;

#[derive(Clone)]
struct H {
    name: String,
    children: Vec<H>,
}

impl H {
    fn is_matching(&self, folder: CustomFolder) {
        assert_eq!(folder.name, self.name, "{}", self.name);
        assert_eq!(folder.children.len(), self.children.len(), "{}", self.name);
        zip(self.children.clone(), folder.children).for_each(|(h, f)| h.is_matching(f));
    }
}

#[test_case(&[], &[]; "empty")]
#[test_case(&[
    ("foo",  None, "foo", 1),
    ("bar",  None, "bar", 2),
    ("titi", None, "titi",5)
], &[H{name: "foo".to_owned(), children: vec![]},
     H{name: "bar".to_owned(), children: vec![]},
     H{name: "titi".to_owned(), children: vec![]}]; "root")]
#[test_case(&[
    ("foo",  None,         "foo",  1),
    ("bar",  Some("foo"),  "bar",  2),
    ("baz",  Some("foo"),  "baz",  3),
    ("toto", Some("baz"),  "toto", 4),
    ("titi", None,         "titi", 5),
    ("tutu", Some("titi"), "tutu", 6),
    ("tata", Some("tutu"), "tata", 7),
    ("tete", Some("tutu"), "tete", 8),
    ("tyty", Some("titi"), "tyty", 9),
], &[H{name: "foo".to_owned(), children: vec![
        H{name: "bar".to_owned(), children: vec![]},
        H{name: "baz".to_owned(), children: vec![
            H{name: "toto".to_owned(), children: vec![]}]},
     ]},
     H{name: "titi".to_owned(), children: vec![
        H{name: "tutu".to_owned(), children: vec![
            H{name: "tata".to_owned(), children: vec![]},
            H{name: "tete".to_owned(), children: vec![]},
        ]},
        H{name: "tyty".to_owned(), children: vec![]},
     ]}]; "hierarchy")]
#[test_case(&[
        ("6S03tXl6_3MgwetdmYXgbiCbWEfLHOLqgfKY4z3vPV_Ev3I_RQ3n7aTKhBir3W-xHo6BEuQVRvQm6mXmqpna-A==", None, "MAILANDR 1533", 1),
        ("0MaEyt75f6VmoD1nRI19sGKGKlZ4Ptc39Q7ADU6U4SL5QKfy_JXIYcyZTkDNqUAI-uNLbiKuK2SMszyCkF3XdA==", None, "attachments", 2),
        ("zccLSjVTgDhH1lNhc4fr3wAJDkmFcfXoHFysiZW5wtqg1QTWz5XjAcP4jE8QqE28Z8ywnRBONH6TiIkik80n4A==", None, "Parent folder", 3),
        ("Wii4_M2KZlr0SahgkFxbbPRli5GBUI958lT7kAH20l-vnMVdIYMFbsiRYrUYkAodN6dDw5-B4f2hpQldAkXMPA==", None, "Fake draft folder", 4),
        ("FowFPbmYw8U1Yfa-ctMx2HKaq4tzPt9IepXWGQHxxOirUMxFsUTthrpY6VA6_x9ofWEBlKmt16D_GBirrlSf1Q==", None, "MAILANDR-1324", 5),
        ("C1-uYFncqGBF2_T2wesNLFICXrjXRzkNMKrGVXv35bQv3_R1PJYNc_K1M7EdjItLiYQBMx8syEfE9xRjGmAEYg==", None, "4434", 6),
        ("MBgNvN-T8OxznT9-yQwuIBOKpql3NOrRHz7_RhVWOxkAO6QEaBUj11ga2QyzS2xZXdwIS-Wfau126IJQHTQZsA==", None, "MAILANDR-1585", 34),
        ("mBXFmQXCKb5EB06VaVpV36P-8z_zLLNp7qDapOfraNWoMskYLWXeuYIgu8I6x4f4v5vUCErukhuZPe1JSJSFKg==", None, "Serdar's embedded images mails", 35),
        ("1srPnwWw8RrslagNnJ96rr4VoANjvSYhjZzeqYG32t1ZiQX-DTgjeX7BocfOd2qK0taXRRWrVADC_LqzT3tuvw==", None, "MAILANDR-1492", 36),
        ("79M8_VJ5uZ_TJknKiP59-C33_5UGs6_RZ_QSEqk1CUFpjKnfxrSGv5JiXysm4PwGxwX-H5C3HzrE7Y-c75IhTg==", None, "409", 37),
        ("atVs_fxpRvotlH34COHvT26g6Rtj1xqe6L6hskDL17RVGAriHTEazsig321xD3_bKZOwJxZs2ooSyoVNoWEaWA==", None, "ET", 42),
        ("8JoGx5N8kqFhoEyh656SyqimAeoDB1X8oXzZC6jVuwb7HqbcF3q7APcOABt_tqNNr2t6nd4RrnxQtd0A_wQVMQ==", None, "2", 47),
        ("rsuf1XfbG6oCxUcKCkJH4NRWdQ5EgATTITBGSbBMlJ4Lpp3FCTg2TczAh6AmpcuH5DOtB7sFFeeRmbnn_usdew==", None, "ET-1123", 49),
        ("UJZduWqNPe86UO-luTfXE0LSoYSD5mK3-Wz-3p0LW1jt9NBDSZDeuNOHLo7ov9q3sEAdywQYA-0XR-mYNkNxPQ==", None, "ET-1135", 50),
        ("yVl6B1xzzKHSR2k8l_7NVpFMxSjvXm0s0MO6yBMOHIpOThCmCT08CNBtBiC2UYk-UdQTmYoJh3qq4ps1jnidug==", Some("28ahuCML8WFhq-NC6YvHA1aHDBHusgnq9K3ETWRgSrue7hB_gs3treB7nEXG62HO_3_-_miiYM9chD_ijQZ8Kw=="), "new folderrrr", 1),
        ("0py8SrtL2-z5c0sWwuXdr3Iw20hCNnyqLLo9Zt9sfVRPCF1KkAMudrWTVqwcLDDoYvkfor8f5CPTassnkspORA==", Some("28ahuCML8WFhq-NC6YvHA1aHDBHusgnq9K3ETWRgSrue7hB_gs3treB7nEXG62HO_3_-_miiYM9chD_ijQZ8Kw=="), "another new folder", 2),
        ("4NfpsVerWGJJ_Q8jbQ7twon7YqwSkQXPMLp7cHFSX18J2LTWS4Qj0USFK6T-chHTdFXwVfeNsxaFuynfAB1FXA==", Some("0MaEyt75f6VmoD1nRI19sGKGKlZ4Ptc39Q7ADU6U4SL5QKfy_JXIYcyZTkDNqUAI-uNLbiKuK2SMszyCkF3XdA=="), "craaaaaash", 1),
        ("28ahuCML8WFhq-NC6YvHA1aHDBHusgnq9K3ETWRgSrue7hB_gs3treB7nEXG62HO_3_-_miiYM9chD_ijQZ8Kw==", Some("0MaEyt75f6VmoD1nRI19sGKGKlZ4Ptc39Q7ADU6U4SL5QKfy_JXIYcyZTkDNqUAI-uNLbiKuK2SMszyCkF3XdA=="), "Folder 3.0.12 30/01", 2),
        ("7F7jUnu0O2lxxPkZClL06Glu4-4sbzc_mUdLiHbC8nFJVUAtjbH8JtMl0FAC0S9r7BcN1YPoSaVOGw65NRtITA==", Some("zccLSjVTgDhH1lNhc4fr3wAJDkmFcfXoHFysiZW5wtqg1QTWz5XjAcP4jE8QqE28Z8ywnRBONH6TiIkik80n4A=="), "Child folder", 1),
        ("MvxuLXOe6OnydwULvZqbihHuxU1UhHjzhsZyWTcarzYEQVx41eI-1-9uNxO1PIvi0gcMmnEcdYOCxGAnZB8fnw==", Some("7F7jUnu0O2lxxPkZClL06Glu4-4sbzc_mUdLiHbC8nFJVUAtjbH8JtMl0FAC0S9r7BcN1YPoSaVOGw65NRtITA=="), "Child folder 2", 1),
        ("3--2i_wtlJ0FqCmqRNMQBxUBtzq21CqrCvartnOkebaJ_KzAVbDTvrZIHMJBl3URDAzuUFZt_znPcI8gvs6iNA==", Some("C1-uYFncqGBF2_T2wesNLFICXrjXRzkNMKrGVXv35bQv3_R1PJYNc_K1M7EdjItLiYQBMx8syEfE9xRjGmAEYg=="), "Newfokdre", 1),
        ("gTF0xQqjKR_9q4Ivp7t62pQOjf8Iz-xm2d_uchw6L36I2tO3FSa60U-sfDECqhPa4r3AhYqv1I5CggqbRAaY3w==", Some("C1-uYFncqGBF2_T2wesNLFICXrjXRzkNMKrGVXv35bQv3_R1PJYNc_K1M7EdjItLiYQBMx8syEfE9xRjGmAEYg=="), "New folder 3", 2),
    ], &[
        H{name: "MAILANDR 1533".to_owned(), children: vec![]},
        H{name: "attachments".to_owned(), children: vec![
            H{name: "craaaaaash".to_owned(), children: vec![]},
            H{name: "Folder 3.0.12 30/01".to_owned(), children: vec![
                H{name: "new folderrrr".to_owned(), children: vec![]},
                H{name: "another new folder".to_owned(), children: vec![]}
            ]},
        ]},
        H{name: "Parent folder".to_owned(), children: vec![
            H{name: "Child folder".to_owned(), children: vec![
                H{name: "Child folder 2".to_owned(), children: vec![]},
            ]},
        ]},
        H{name: "Fake draft folder".to_owned(), children: vec![]},
        H{name: "MAILANDR-1324".to_owned(), children: vec![]},
        H{name: "4434".to_owned(), children: vec![
            H{name: "Newfokdre".to_owned(), children: vec![]},
            H{name: "New folder 3".to_owned(), children: vec![]}
        ]},
        H{name: "MAILANDR-1585".to_owned(), children: vec![]},
        H{name: "Serdar's embedded images mails".to_owned(), children: vec![]},
        H{name: "MAILANDR-1492".to_owned(), children: vec![]},
        H{name: "409".to_owned(), children: vec![]},
        H{name: "ET".to_owned(), children: vec![]},
        H{name: "2".to_owned(), children: vec![]},
        H{name: "ET-1123".to_owned(), children: vec![]},
        H{name: "ET-1135".to_owned(), children: vec![]},
    ]; "Bug ET-1101")]
#[tokio::test]
async fn sidebar_custom_folders(labels: &[(&str, Option<&str>, &str, u32)], expected: &[H]) {
    // Setup:
    //   * Setup User:
    //     + Create Custom Folders
    //   * Create Sidebar
    let ctx = MailTestContext::new().await;
    ctx.setup_user(sidebar_test_params(labels)).await;

    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;

    let stash = user_ctx.user_stash();
    let tether = stash.connection().await.unwrap();

    // Action
    let result = Sidebar.custom_folders(&tether).await.unwrap();

    // Tests
    for (res, h) in zip(result, expected) {
        h.is_matching(res);
    }
}

fn sidebar_test_params(labels: &[(&str, Option<&str>, &str, u32)]) -> TestParams {
    TestParams {
        labels: hash_map! { LabelType::Folder: labels.iter().map(create_label).collect()},
        ..Default::default()
    }
}

fn create_label((id, parent_id, name, order): &(&str, Option<&str>, &str, u32)) -> ApiLabel {
    ApiLabel {
        id: LabelId::from(*id),
        parent_id: parent_id.map(LabelId::from),
        color: "".to_string(),
        display: false,
        expanded: false,
        label_type: LabelType::Folder,
        name: name.to_owned().to_owned(),
        notify: false,
        order: order.to_owned(),
        path: None,
        sticky: false,
    }
}
