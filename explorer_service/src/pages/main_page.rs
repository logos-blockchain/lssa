use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_query_map};
use web_sys::SubmitEvent;

use crate::{
    api::{self, SearchResults},
    components::{AccountPreview, BlockPreview, TransactionPreview},
};

const RECENT_BLOCKS_LIMIT: u64 = 10;

/// Main page component
#[component]
pub fn MainPage() -> impl IntoView {
    let query_map = use_query_map();
    let navigate = use_navigate();

    // Read search query from URL parameter
    let url_query = move || query_map.read().get("q").unwrap_or_default();

    let (search_query, set_search_query) = signal(url_query());

    // Sync search input with URL parameter
    Effect::new(move || {
        set_search_query.set(url_query());
    });

    // Search results resource based on URL query parameter
    let search_resource = Resource::new(url_query, |query| async move {
        if query.is_empty() {
            return None;
        }
        match api::search(query).await {
            Ok(result) => Some(result),
            Err(e) => {
                log::error!("Search error: {}", e);
                None
            }
        }
    });

    // Load recent blocks on mount
    let recent_blocks_resource = Resource::new(
        || (),
        |_| async {
            match api::get_latest_block_id().await {
                Ok(last_id) => {
                    api::get_blocks(
                        std::cmp::max(last_id.saturating_sub(RECENT_BLOCKS_LIMIT) as u32, 1),
                        RECENT_BLOCKS_LIMIT as u32,
                    )
                    .await
                }
                Err(err) => Err(err),
            }
        },
    );

    // Handle search - update URL parameter
    let on_search = move |ev: SubmitEvent| {
        ev.prevent_default();
        let query = search_query.get();
        if query.is_empty() {
            navigate("?", Default::default());
            return;
        }

        navigate(
            &format!("?q={}", urlencoding::encode(&query)),
            Default::default(),
        );
    };

    view! {
        <div class="main-page">
            <div class="page-header">
                <h1>"LEZ Block Explorer"</h1>
            </div>

            <div class="search-section">
                <form on:submit=on_search class="search-form">
                    <input
                        type="text"
                        class="search-input"
                        placeholder="Search by block ID, block hash, transaction hash, or account ID..."
                        prop:value=move || search_query.get()
                        on:input=move |ev| set_search_query.set(event_target_value(&ev))
                    />
                    <button type="submit" class="search-button">
                        "Search"
                    </button>
                </form>

                <Suspense fallback=move || view! { <div class="loading">"Searching..."</div> }>
                    {move || {
                        search_resource
                            .get()
                            .and_then(|opt_results| opt_results)
                            .map(|results| {
                                let SearchResults {
                                    blocks,
                                    transactions,
                                    accounts,
                                } = results;
                                let has_results = !blocks.is_empty()
                                    || !transactions.is_empty()
                                    || !accounts.is_empty();
                                view! {
                                    <div class="search-results">
                                        <h2>"Search Results"</h2>
                                        {if !has_results {
                                            view! { <div class="not-found">"No results found"</div> }
                                            .into_any()
                                    } else {
                                        view! {
                                            <div class="results-container">
                                                {if !blocks.is_empty() {
                                                    view! {
                                                        <div class="results-section">
                                                            <h3>"Blocks"</h3>
                                                            <div class="results-list">
                                                                {blocks
                                                                    .into_iter()
                                                                    .map(|block| {
                                                                        view! { <BlockPreview block=block /> }
                                                                    })
                                                                    .collect::<Vec<_>>()}
                                                            </div>
                                                        </div>
                                                    }
                                                        .into_any()
                                                } else {
                                                    ().into_any()
                                                }}

                                                {if !transactions.is_empty() {
                                                    view! {
                                                        <div class="results-section">
                                                            <h3>"Transactions"</h3>
                                                            <div class="results-list">
                                                                {transactions
                                                                    .into_iter()
                                                                    .map(|tx| {
                                                                        view! { <TransactionPreview transaction=tx /> }
                                                                    })
                                                                    .collect::<Vec<_>>()}
                                                            </div>
                                                        </div>
                                                    }
                                                        .into_any()
                                                } else {
                                                    ().into_any()
                                                }}

                                                {if !accounts.is_empty() {
                                                    view! {
                                                        <div class="results-section">
                                                            <h3>"Accounts"</h3>
                                                            <div class="results-list">
                                                                {accounts
                                                                    .into_iter()
                                                                    .map(|(id, account)| {
                                                                        view! {
                                                                            <AccountPreview
                                                                                account_id=id
                                                                                account=account
                                                                            />
                                                                        }
                                                                    })
                                                                    .collect::<Vec<_>>()}
                                                            </div>
                                                        </div>
                                                    }
                                                        .into_any()
                                                } else {
                                                    ().into_any()
                                                }}

                                            </div>
                                        }
                                            .into_any()
                                    }}
                                </div>
                            }
                                .into_any()
                        })
                    }}

                </Suspense>
            </div>

            <div class="blocks-section">
                <h2>"Recent Blocks"</h2>
                <Suspense fallback=move || view! { <div class="loading">"Loading blocks..."</div> }>
                    {move || {
                        recent_blocks_resource
                            .get()
                            .map(|result| match result {
                                Ok(blocks) if !blocks.is_empty() => {
                                    view! {
                                        <div class="blocks-list">
                                            {blocks
                                                .into_iter()
                                                .map(|block| view! { <BlockPreview block=block /> })
                                                .collect::<Vec<_>>()}
                                        </div>
                                    }
                                        .into_any()
                                }
                                Ok(_) => {
                                    view! { <div class="no-blocks">"No blocks found"</div> }.into_any()
                                }
                                Err(e) => {
                                    view! { <div class="error">{format!("Error: {}", e)}</div> }
                                        .into_any()
                                }
                            })
                    }}

                </Suspense>
            </div>
        </div>
    }
}
