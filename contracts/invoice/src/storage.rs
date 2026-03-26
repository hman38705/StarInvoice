use soroban_sdk::{contracttype, Address, Env, String};
use crate::constants::{TTL_THRESHOLD, TTL_EXTEND_TO};

/// Represents the lifecycle state of an invoice.
#[contracttype]
#[derive(Clone, PartialEq, Debug)]
pub enum InvoiceStatus {
    /// Invoice created, awaiting client funding.
    Pending,
    /// Client has deposited funds into escrow.
    Funded,
    /// Freelancer has marked work as delivered.
    Delivered,
    /// Client disputes the invoice.
    Disputed,
    /// Client has approved the delivery.
    Approved,
    /// Funds have been released to the freelancer.
    Completed,
    /// Invoice has been voided by the freelancer or client.
    Cancelled,
}

/// Core invoice data structure stored on-chain.
#[contracttype]
#[derive(Clone)]
pub struct Invoice {
    /// Unique numeric identifier for this invoice.
    pub id: u64,
    /// Address of the freelancer who created the invoice.
    pub freelancer: Address,
    /// Address of the client responsible for funding.
    pub client: Address,
    /// Payment amount in the smallest token unit (stroops).
    pub amount: i128,
    /// Human-readable description of the work to be performed.
    pub description: String,
    /// Address of the token contract used for payment.
    pub token: Address,
    /// Unix timestamp after which the invoice can no longer be funded.
    pub deadline: u64,
    /// Unix timestamp when the invoice was created.
    pub created_at: u64,
    /// Current state of the invoice in the escrow lifecycle.
    pub status: InvoiceStatus,
}

#[contracttype]
enum DataKey {
    Invoice(u64),
    InvoiceCount,
}

/// Returns the current invoice count.
pub fn get_invoice_count(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::InvoiceCount)
        .unwrap_or(0)
}

/// Returns the next available invoice ID and increments the counter.
///
/// Storage: persistent — the counter must survive contract upgrades and
/// instance expiry. Losing it would cause ID collisions with existing invoices.
pub fn next_invoice_id(env: &Env) -> u64 {
    let count: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::InvoiceCount)
        .unwrap_or(0);
    env.storage()
        .persistent()
        .set(&DataKey::InvoiceCount, &(count + 1));
    count
}

/// Persists an invoice to on-chain storage, keyed by its ID.
/// 
/// This function also extends the TTL of the storage entry to prevent
/// it from being evicted from persistent storage.
pub fn save_invoice(env: &Env, invoice: &Invoice) {
    let key = DataKey::Invoice(invoice.id);
    env.storage()
        .persistent()
        .set(&key, invoice);
    
    // Extend TTL to prevent storage eviction
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

/// Retrieves an invoice by ID. Panics if the invoice does not exist.
/// 
/// This function also extends the TTL of the storage entry to prevent
/// it from being evicted from persistent storage.
pub fn get_invoice(env: &Env, invoice_id: u64) -> Invoice {
    let key = DataKey::Invoice(invoice_id);
    let invoice = env.storage()
        .persistent()
        .get(&key)
        .expect("Invoice not found");
    
    // Extend TTL to prevent storage eviction
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    
    invoice
}
