use axum::{Router, routing::post};

pub fn routes() -> Router {
    Router::new().route("/index", post(index_handler))
}

async fn index_handler() -> &'static str {
    "TODO"
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::StatusCode;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_index_endpoint_exists() {
        let app = routes();

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/index")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
