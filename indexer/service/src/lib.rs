#[cfg(not(feature = "mock-responses"))]
pub mod service;

#[cfg(feature = "mock-responses")]
pub mod mock_service;
