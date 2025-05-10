use std::sync::Arc;

use lambda_runtime::{service_fn, LambdaEvent, Error};
use serde_json::{json, Value};

pub struct State {

}

impl State {
    fn new() -> Self { Self {} }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let state = Arc::new(State::new());
    let func = service_fn(move |event| {
        // // Clone Arc to pass the handler to each request
        let state = Arc::clone(&state);
        // async move { handler.handle(event).await }
        entrypoint(state, event)
    });
    // let func = service_fn(func);
    lambda_runtime::run(func).await?;
    Ok(())
}

async fn entrypoint(_state: Arc<State>, event: LambdaEvent<Value>) -> Result<Value, Error> {
    let (event, _context) = event.into_parts();
    let first_name = event["firstName"].as_str().unwrap_or("world");
    println!("{:?}", event);

    Ok(json!({ "message": format!("Hello, {}!", first_name) }))
}
