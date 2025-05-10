pub const PKEY: &'static str = "PKEY";
pub const SKEY: &'static str = "SKEY";

pub fn matchmaking_pkey(turn_number: u32) -> String {
    format!("matchmaking_turn_{}", turn_number)
}
