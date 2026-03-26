use soroban_sdk::{contracterror, contracttype, Address, Env, String};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    InvoiceNotFound = 1,
    ClientIsFreelancer = 2,
}

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
/// Storage: persistent — invoices hold escrowed funds and must remain
/// accessible for the full escrow lifecycle, independent of instance TTL.
pub fn save_invoice(env: &Env, invoice: &Invoice) {
    env.storage()
        .persistent()
        .set(&DataKey::Invoice(invoice.id), invoice);
}

/// Retrieves an invoice by ID. Returns Result::Err if not found.
pub fn get_invoice(env: &Env, invoice_id: u64) -> Result<Invoice, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::Invoice(invoice_id))
        .ok_or(ContractError::InvoiceNotFound)
}
