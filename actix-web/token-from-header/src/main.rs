#![allow(clippy::needless_lifetimes)]

use actix_web::{guard, web, App, HttpRequest, HttpResponse, HttpServer, Result};
use actix_web_actors::ws;
use async_graphql::http::{playground_source, GraphQLPlaygroundConfig};
use async_graphql::{
    Context, Data, EmptyMutation, FieldResult, GQLObject, GQLSubscription, Schema,
};
use async_graphql_actix_web::{GQLRequest, GQLResponse, WSSubscription};
use futures::{stream, Stream};

type MySchema = Schema<QueryRoot, EmptyMutation, SubscriptionRoot>;

struct MyToken(String);

struct QueryRoot;

#[GQLObject]
impl QueryRoot {
    async fn current_token<'a>(&self, ctx: &'a Context<'_>) -> Option<&'a str> {
        ctx.data_opt::<MyToken>().map(|token| token.0.as_str())
    }
}

struct SubscriptionRoot;

#[GQLSubscription]
impl SubscriptionRoot {
    async fn values(&self, ctx: &Context<'_>) -> FieldResult<impl Stream<Item = i32>> {
        if ctx.data_unchecked::<MyToken>().0 != "123456" {
            return Err("Forbidden".into());
        }
        Ok(stream::once(async move { 10 }))
    }
}

async fn index(
    schema: web::Data<MySchema>,
    req: HttpRequest,
    gql_request: GQLRequest,
) -> GQLResponse {
    let token = req
        .headers()
        .get("Token")
        .and_then(|value| value.to_str().map(|s| MyToken(s.to_string())).ok());
    let mut request = gql_request.into_inner();
    if let Some(token) = token {
        request = request.data(token);
    }
    schema.execute(request).await.into()
}

async fn gql_playgound() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(playground_source(
            GraphQLPlaygroundConfig::new("/").subscription_endpoint("/"),
        ))
}

async fn index_ws(
    schema: web::Data<MySchema>,
    req: HttpRequest,
    payload: web::Payload,
) -> Result<HttpResponse> {
    ws::start_with_protocols(
        WSSubscription::new(&schema).initializer(|value| {
            #[derive(serde_derive::Deserialize)]
            struct Payload {
                token: String,
            }

            if let Ok(payload) = serde_json::from_value::<Payload>(value) {
                let mut data = Data::default();
                data.insert(MyToken(payload.token));
                Ok(data)
            } else {
                Err("Token is required".into())
            }
        }),
        &["graphql-ws"],
        &req,
        payload,
    )
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    let schema = Schema::new(QueryRoot, EmptyMutation, SubscriptionRoot);

    println!("Playground: http://localhost:8000");

    HttpServer::new(move || {
        App::new()
            .data(schema.clone())
            .service(web::resource("/").guard(guard::Post()).to(index))
            .service(
                web::resource("/")
                    .guard(guard::Get())
                    .guard(guard::Header("upgrade", "websocket"))
                    .to(index_ws),
            )
            .service(web::resource("/").guard(guard::Get()).to(gql_playgound))
    })
    .bind("127.0.0.1:8000")?
    .run()
    .await
}
