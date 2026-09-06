fn main() {
    println!(
        "{}",
        serde_json::to_string_pretty(&cortex_app_server::api::schema::document()).unwrap()
    );
}
