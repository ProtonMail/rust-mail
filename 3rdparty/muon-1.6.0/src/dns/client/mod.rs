crate::if_not_wasm! {
    crate::if_dns_client! {
        export! { mod dns (as pub); }
    }

    crate::if_doh_client! {
        export! { mod doh (as pub); }
    }
}
