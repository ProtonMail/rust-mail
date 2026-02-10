if_not_wasm! {
    if_dns_client! {
        export! { mod dns (as pub); }
    }

    if_doh_client! {
        export! { mod doh (as pub); }
    }
}
