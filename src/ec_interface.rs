// for now - to be a SHA of the public-key - so 256 bit
pub type PublicKeyReference = u64;
pub type Signature = u64;

// all the same numeric type of some size to allow casting/interop
pub type PeerId = u64;
pub type TokenId = PeerId;
pub type BlockId = PeerId;

pub type EcTime = u64;
pub type MessageTicket = u64;

pub const TOKENS_PER_BLOCK: usize = 6;
pub const TOKENS_SIGNATURE_SIZE: usize = 8;

// block can not claim to be further into the future
pub const SOME_STEPS_INTO_THE_FUTURE: EcTime = 100;

pub const VOTE_THRESHOLD: i64 = 1;

// TODO bad name
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct TokenBlock {
    pub token: TokenId,
    pub last: BlockId,
    pub key: PublicKeyReference,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Block {
    pub id: BlockId,
    pub time: EcTime,
    pub used: u8,
    pub parts: [TokenBlock; TOKENS_PER_BLOCK],

    // Not part of the block it-self (that is not part of the id)
    pub signatures: [Option<Signature>; TOKENS_PER_BLOCK],
}

#[derive(Clone)]
pub struct TokenMapping {
    pub id: TokenId,
    pub block: BlockId,
}

// TODO make group message of Submit, Query and Validate
#[derive(Clone)]
pub enum Message {
    Vote {
        block_id: BlockId,
        vote: u8,
        reply: bool,
    },
    Query {
        token: TokenId,
        target: PeerId, // who wants the result? 0 => me (in network case this allow NAT discovery of peer address)
        ticket: MessageTicket,
    },
    Answer {
        answer: TokenMapping,
        signature: [TokenMapping; TOKENS_SIGNATURE_SIZE],
    },
    Block {
        block: Block,
    },
}

pub enum RequestMessage {
    Vote {
        block: BlockId,
        status: [bool; TOKENS_PER_BLOCK],
    },
    Query {
        token: TokenId,
        target: PeerId,
    },
    Empty,
}

pub enum Message2 {
    Answer {
        answer: TokenMapping,
        signature: [TokenMapping; TOKENS_SIGNATURE_SIZE],
    },
    Block {
        block: Block,
    },
    Requests {
        messages: [RequestMessage; TOKENS_SIGNATURE_SIZE],
    },
}

#[derive(Clone)]
pub struct MessageEnvelope {
    pub sender: PeerId,
    pub receiver: PeerId,
    pub ticket: MessageTicket,
    pub time: EcTime,
    pub message: Message,
}

///
/// Database type
///
///

pub struct BlockTime {
    pub(crate) block: BlockId,
    pub(crate) time: EcTime,
}

pub trait EcTokens {
    fn lookup(&self, token: &TokenId) -> Option<&BlockTime>;

    fn set(&mut self, token: &TokenId, block: &BlockId, time: EcTime);

    /// Challenge: Find the smallest expand around token with tokenIds ending on bytes
    /// matching the bytes in the key
    /// Also tokenmappings must not point to blocks older than some threshold
    ///
    fn tokens_signature(&self, token: &TokenId, key: &PeerId) -> Option<Message>;
}

pub trait EcBlocks {
    fn lookup(&self, block: &BlockId) -> Option<Block>;

    fn exists(&self, block: &BlockId) -> bool;

    fn save(&mut self, block: &Block);

    fn remove(&mut self, block: &BlockId);
}
