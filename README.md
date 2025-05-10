# arena multiplayer

a framework for async multiplayer games with a focus on "arena" modes similar to [super auto pets arena mode](https://superautopets.wiki.gg/wiki/The_Basics#Arena).

## structure

- `shared` shared crate with common definitions needed across deployment time, runtime, and the frontend
- `logic` core dynamodb logic for matchmaking, turn handling
- `deploy` code used to deploy to AWS. this uses a private dependency `ensko` which you wont be able to run, but the infrastructure is simple enough to recreate.
- 
