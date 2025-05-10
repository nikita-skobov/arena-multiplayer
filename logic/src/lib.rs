use std::str::FromStr;

use aws_sdk_dynamodb::{types::{AttributeValue, Delete, TransactWriteItem}, Client};
use shared::{PKEY, SKEY};


#[derive(Debug)]
pub enum MatchResult {
    UnrecoverableError(String),
    P1ConditionError,
    P2ConditionError,
    Matched(MatchmakingSkey, MatchmakingSkey),
}

pub enum MatchmakingResult {
    Matched(MatchmakingSkey),
    /// if Some(string) => there was an unknown error causing us to fake simulate
    /// if None => there were no other players to match against, so we fake simulate
    FakeSimulate(Option<String>),
    /// this happens if our player was already matched by another invocation. we can
    /// drop this request as they were already matched
    CanDrop,
}

pub struct AsyncMatchmakingRequest {
    pub turn_number: u32,
    pub skey: MatchmakingSkey,
}

#[derive(Debug, Clone)]
pub struct MatchmakingSkey {
    pub random_component: String,
    pub run_id: String,
}

impl MatchmakingSkey {
    pub fn new(run_id: String) -> Self {
        Self { random_component: get_random_string(16), run_id }
    }
    pub fn format(&self) -> String {
        format!("{}_{}", self.random_component, self.run_id)
    }
}

impl FromStr for MatchmakingSkey {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (random_component, run_id) = match s.split_once("_") {
            Some((l, r)) => (l.to_string(), r.to_string()),
            _ => return Err(format!("failed to detect MatchmakingSkey from '{}'", s)),
        };
        Ok(Self { random_component, run_id })
    }
}


#[allow(deprecated)]
pub async fn get_client() -> Client {
    let config = aws_config::load_from_env().await;
    aws_sdk_dynamodb::Client::new(&config)
}

/// only fetches one page instead of paginating. reason is
/// we only care about matchmaking 1:1, therefore no reason to get every single possible opponent.
/// also, the sort keys have a random prefix, which should make the sorting random.
pub async fn list_matchmaking_entries(
    ddb_client: &Client,
    table_name: &str,
    turn_number: u32
) -> Result<Vec<MatchmakingSkey>, String> {
    let out = ddb_client.query()
        .table_name(table_name)
        .key_condition_expression(format!("{} = :pkey", PKEY))
        .expression_attribute_values(":pkey", AttributeValue::S(shared::matchmaking_pkey(turn_number)))
        .send().await.map_err(|e| e.to_string())?;
    let items = out.items().to_vec(); 
    let mut out_items = Vec::with_capacity(items.len());
    for mut item in items {
        let skey = item.remove(SKEY).ok_or(&format!("failed to find '{}' sort key", SKEY))?;
        let skey_value = skey.as_s().map_err(|e| format!("incorrect attr type for {}: {:?}", SKEY, e))?;
        let matchmakingskey = MatchmakingSkey::from_str(&skey_value)?;
        out_items.push(matchmakingskey);
    }
    Ok(out_items)
}

pub fn get_random_string(num: usize) -> String {
    let mut out = String::with_capacity(num);
    for _ in 0..num {
        let c = fastrand::char('a'..'z');
        out.push(c);
    }
    out
}

pub async fn end_turn(
    ddb_client: &Client,
    table_name: &str,
    turn_number: u32,
    run_id: String,
) -> Result<MatchmakingSkey, String> {
    let skey = MatchmakingSkey::new(run_id);
    ddb_client.put_item()
        .table_name(table_name)
        .item(PKEY, AttributeValue::S(shared::matchmaking_pkey(turn_number)))
        .item(SKEY, AttributeValue::S(skey.format()))
        // this is unlikely to happen as we have a random component, but just in case:
        .condition_expression(format!("attribute_not_exists({PKEY})"))
        .send().await.map_err(|e| format!("Failed to end turn: {:?}", e))?;
    Ok(skey)
}

pub async fn attempt_match(
    ddb_client: &Client,
    table_name: &str,
    turn_number: u32,
    player1: MatchmakingSkey,
    player2: MatchmakingSkey,
) -> MatchResult {
    let delete1 = Delete::builder()
        .table_name(table_name)
        .key(PKEY, AttributeValue::S(shared::matchmaking_pkey(turn_number)))
        .key(SKEY, AttributeValue::S(player1.format()))
        .condition_expression(format!("attribute_exists({PKEY})"))
        .return_values_on_condition_check_failure(aws_sdk_dynamodb::types::ReturnValuesOnConditionCheckFailure::AllOld)
        .build().expect("transaction builder failure!");

    let delete2 = Delete::builder()
        .table_name(table_name)
        .key(PKEY, AttributeValue::S(shared::matchmaking_pkey(turn_number)))
        .key(SKEY, AttributeValue::S(player2.format()))
        .condition_expression(format!("attribute_exists({PKEY})"))
        .return_values_on_condition_check_failure(aws_sdk_dynamodb::types::ReturnValuesOnConditionCheckFailure::AllOld)
        .build().expect("transaction builder failure!");

    let resp = ddb_client
        .transact_write_items()
        .transact_items(TransactWriteItem::builder().delete(delete1).build())
        .transact_items(TransactWriteItem::builder().delete(delete2).build())        
        .send()
        .await;
    match resp {
        Ok(_) => MatchResult::Matched(player1, player2),
        Err(e) => {
            if let Some(transact_err) = e.as_service_error() {
                match transact_err {
                    aws_sdk_dynamodb::operation::transact_write_items::TransactWriteItemsError::TransactionCanceledException(transaction_canceled_exception) => {
                        let reasons = transaction_canceled_exception.cancellation_reasons.clone().unwrap_or_default();
                        let reason1 = reasons.get(0).and_then(|x| Some(x.message.is_some()));
                        let reason2 = reasons.get(1).and_then(|x| Some(x.message.is_some()));
                        if reason1 == Some(true) {
                            // condition error on p1
                            // this should trump condition error on p2
                            MatchResult::P1ConditionError
                        } else if reason2 == Some(true) {
                            MatchResult::P2ConditionError
                        } else {
                            // not sure what can cause this, but we treat it as unrecoverable just in case
                            MatchResult::UnrecoverableError(transaction_canceled_exception.to_string())
                        }
                    }
                    e => MatchResult::UnrecoverableError(e.to_string()),
                }
            } else {
                MatchResult::UnrecoverableError(e.to_string())
            }
        }
    }
}

pub async fn attempt_matchmaking(
    ddb_client: &Client,
    table_name: &str,
    player1: AsyncMatchmakingRequest,
) -> Result<MatchmakingResult, String> {
    let mut available_opponents = list_matchmaking_entries(ddb_client, table_name, player1.turn_number).await?;
    // prevent matching against self!
    available_opponents.retain(|x| x.run_id != player1.skey.run_id);

    let AsyncMatchmakingRequest { turn_number, skey } = player1;
    for op in available_opponents {
        match attempt_match(ddb_client, table_name, turn_number, skey.clone(), op).await {
            MatchResult::P2ConditionError => {},
            MatchResult::P1ConditionError => return Ok(MatchmakingResult::CanDrop),
            MatchResult::UnrecoverableError(e) => return Ok(MatchmakingResult::FakeSimulate(Some(e))),
            MatchResult::Matched(_, matchmaking_skey) => {
                return Ok(MatchmakingResult::Matched(matchmaking_skey))
            }
        }
    }

    // if we get here it means we ran out of opponents to match against (or there were none)
    // so we should simulate a fake opponent for the matchmaking
    Ok(MatchmakingResult::FakeSimulate(None))
}

// end turn => submit matchmaking item: PKEY:turn_X, SKEY:{some_id}, idempotency: {random}
// async matchmaking => query all turn_X
// pick best partner
// delete matchmaking items for both players:
// transaction pt1: Delete PKEY:turn_x, SKEY:{some_id}, condition: PKEY exists
// transaction pt2: Delete PKEY:turn_x, SKEY:{...}, condition: PKEY exists (prevent deletion returning success if this item already doesnt exist)
// if successful: queue simulation task for {some_id} and {...} which will update those items respectively, but without transaction
// if error due to condition failure on pt2: try again with a different user
// if error due to condition failure on pt1: it was already assigned by a different invocation, therefore mission accomplished, no need to enqueue anything
// if error otherwise: log the error, enqueue a simulation against a fake opponent, log a metric that the user played against a fake player. we want to track this and minimize it

#[cfg(test)]
mod test {
    use super::*;

    const TC_TABLE: &'static str = "mygametable2025";

    macro_rules! tc {
        ($name:ident; |$c:ident| { $($x:tt)*}) => {
            #[test]
            fn $name() {
                let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("aa");
                rt.block_on(async {
                    let client = get_client().await;
                    let $c = &client;
                    $($x)*
                });                
            }
        };
    }

    tc!(matchmaking_happy_path_works; |c| {
        let player1 = end_turn(c, TC_TABLE, 1, "a".to_string()).await.expect("failed to end turn");
        let player2 = end_turn(c, TC_TABLE, 1, "b".to_string()).await.expect("failed to end turn");
        let res = attempt_match(c, TC_TABLE, 1, player1, player2).await;
        match res {
            MatchResult::Matched(p1, p2) => {
                assert_eq!(p1.run_id, "a");
                assert_eq!(p2.run_id, "b");
            }
            e => panic!("Unexpected result: {:?}", e),
        }
    });

    tc!(matchmaking_can_report_if_p2_already_matched; |c| {
        let player1 = end_turn(c, TC_TABLE, 1, "a".to_string()).await.expect("failed to end turn");
        // player2 doesnt exist in the table. we should get a player2 condition error if we try to matchmake:
        let player2 = MatchmakingSkey::new("b".to_string());
        let res = attempt_match(c, TC_TABLE, 1, player1, player2).await;
        match res {
            MatchResult::P2ConditionError => {}
            e => panic!("Unexpected result: {:?}", e),
        }
    });

    tc!(matchmaking_can_report_if_p1_already_matched; |c| {
        // player1 doesnt exist in the table. we should get a player1 condition error if we try to matchmake:
        let player1 = MatchmakingSkey::new("a".to_string());
        let player2 = end_turn(c, TC_TABLE, 1, "b".to_string()).await.expect("failed to end turn");
        let res = attempt_match(c, TC_TABLE, 1, player1, player2).await;
        match res {
            MatchResult::P1ConditionError => {}
            e => panic!("Unexpected result: {:?}", e),
        }
    });

    tc!(matchmaking_can_report_unknown_errors; |c| {
        // the table doesnt exist, so we should get an unexpected error
        let player1 = MatchmakingSkey::new("a".to_string());
        let player2 = MatchmakingSkey::new("a".to_string());
        let res = attempt_match(c, "eeeeeeeeefaketable", 1, player1, player2).await;
        match res {
            MatchResult::UnrecoverableError(e) => {
                assert!(e.contains("ResourceNotFoundException"), "{:?}", e);
            }
            e => panic!("Unexpected result: {:?}", e),
        }
    });
}
