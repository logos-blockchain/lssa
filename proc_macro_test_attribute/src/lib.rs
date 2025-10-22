extern crate proc_macro;

use proc_macro::*;

#[proc_macro_attribute]
pub fn test_suite_fn(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = item.to_string();

    let fn_keyword = "fn ";
    let fn_keyword_alternative = "fn\n";

    let mut start_opt = None;
    let mut fn_name = String::new();

    if let Some(start) = input.find(fn_keyword) {
        start_opt = Some(start);
    } else if let Some(start) = input.find(fn_keyword_alternative) {
        start_opt = Some(start);
    }

    if let Some(start) = start_opt {
        let rest = &input[start + fn_keyword.len()..];
        if let Some(end) = rest.find(|c: char| c == '(' || c.is_whitespace()) {
            let name = &rest[..end];
            fn_name = name.to_string();
        }
    } else {
        println!("ERROR: keyword fn not found");
    }

    let extension = format!(
        r#"
    {input}

    function_map.insert("{fn_name}".to_string(), |home_dir: PathBuf| Box::pin(async {{
        let res = pre_test(home_dir).await.unwrap();

        info!("Waiting for first block creation");
        tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

        {fn_name}().await;

        post_test(res).await;
    }}));
    "#
    );

    extension.parse().unwrap()
}
