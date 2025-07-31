use proton_core_common::datatypes::AppDetails;

pub fn new_app_details(platform: String, product: String, version: String) -> AppDetails {
    AppDetails {
        platform,
        product,
        version,
    }
}
