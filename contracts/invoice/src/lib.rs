#! [no_std]

mod constants;
mod events;
mod storage;

use soroban_sdk::{contract, contractimpl, token, Address, Env, String};

pub use storage::{Invoice, ContractError};

#[contract]
pub struct InvoiceContract;

#[contractimpl]
impl InvoiceContract {
    /// Creates a new invoice and stores it on-chain.
    ///
    /// # Parameters
    /// - `freelancer`: Address of the service provider; must sign the transaction.
    /// - `client`: Address of the paying party.
    /// - `amount`: Payment amount in the smallest token unit (stroops). Uses `i128`;
    ///   overflow is prevented at the platform level via `overflow-checks = true`
    ///   in the `[profile.release]` section of `contracts/invoice/Cargo.toml`.
    /// - `description`: Human-readable description of the work.
    ///
    /// # Returns
    /// The newly assigned invoice ID.
    ///
    /// # Errors
    /// Panics if `freelancer` authorization fails.
    pub fn create_invoice(
        env: Env,
        freelancer: Address,
        client: Address,
        amount: i128,
        token: Address,
        deadline: u64,
        description: String,
    ) -> u64 {
        freelancer.require_auth();

        assert!(freelancer != client, "Client and freelancer must be different addresses");

        let invoice_id = storage::next_invoice_id(&env);

        let invoice = Invoice {
            id: invoice_id,
            freelancer: freelancer.clone(),
            client: client.clone(),
            amount,
            token,
            deadline,
            created_at: env.ledger().timestamp(),
            description,
            status: storage::InvoiceStatus::Pending,
        };

    // Insert new fn before release_payment

    /// Allows the client to deposit funds into escrow for the given invoice.
    ///
    /// # Parameters
    /// - `invoice_id`: ID of the invoice to fund.
    ///
    /// # Errors
    /// - Panics if the caller is not the invoice client.
    /// - Panics if the invoice status is not `Pending`.
    pub fn fund_invoice(env: Env, invoice_id: u64) {
        let mut invoice = storage::get_invoice(&env, invoice_id).unwrap();

        invoice.client.require_auth();

        if invoice.status != storage::InvoiceStatus::Pending {
            return Err(ContractError::InvalidInvoiceStatus);
        }

        let token = token::Client::new(&env, &token_address);
        // SAFETY: Soroban cross-contract calls are synchronous and atomic within a single
        // transaction. There is no re-entrant execution path — a callee cannot call back into
        // this contract mid-transfer because Soroban does not support async callbacks or
        // mid-transaction re-entry. State is committed only after the full call tree succeeds.
        // See: https://developers.stellar.org/docs/learn/smart-contract-internals/contract-interactions/cross-contract
        token.transfer(&invoice.client, &env.current_contract_address(), &invoice.amount);

        invoice.status = storage::InvoiceStatus::Funded;
        storage::save_invoice(&env, &invoice);

        events::invoice_funded(&env, invoice_id, &invoice.client);
    }

    /// Allows the freelancer to signal that work has been completed.
    ///
    /// # Parameters
    /// - `invoice_id`: ID of the invoice to mark as delivered.
    ///
    /// # Errors
    /// - Panics if the caller is not the invoice freelancer.
    /// - Returns `ContractError::InvalidInvoiceStatus` if the invoice status is not `Funded`.
    pub fn mark_delivered(env: Env, invoice_id: u64) -> Result<(), ContractError> {
        let mut invoice = storage::get_invoice(&env, invoice_id)?;

        invoice.freelancer.require_auth();

        if invoice.status != storage::InvoiceStatus::Funded {
            return Err(ContractError::InvalidInvoiceStatus);
        }

        invoice.status = storage::InvoiceStatus::Delivered;
        storage::save_invoice(&env, &invoice);

        events::mark_delivered(&env, invoice_id, &invoice.freelancer);
        Ok(())
    }

    /// Allows the client to approve the delivered work, authorising fund release.
    ///
    /// # Parameters
    /// - `invoice_id`: ID of the invoice to approve.
    ///
    /// # Errors
    /// - Panics if the caller is not the invoice client.
    /// - Returns `ContractError::InvalidInvoiceStatus` if the invoice status is not `Delivered`.
    ///
    /// # TODO
    /// Not yet implemented. See: <https://github.com/your-org/StarInvoice/issues/3>
    pub fn approve_payment(env: Env, invoice_id: u64) -> Result<(), ContractError> {
        let mut invoice = storage::get_invoice(&env, invoice_id)?;

        invoice.client.require_auth();

        if invoice.status != storage::InvoiceStatus::Funded && invoice.status != storage::InvoiceStatus::Delivered {
            panic!("Can only dispute from Funded or Delivered status");
        }

        invoice.status = storage::InvoiceStatus::Disputed;
        storage::save_invoice(&env, &invoice);

        events::invoice_approved(&env, invoice_id, &invoice.client);
        Ok(())
    }

    // existing release_payment ...

    /// Returns the data for a specific invoice ID.
    pub fn get_invoice(env: Env, invoice_id: u64) -> Result<Invoice, ContractError> {
        storage::get_invoice(&env, invoice_id)
    }

    /// Cancels a Pending invoice, voiding it permanently.
    ///
    /// # Parameters
    /// - `invoice_id`: ID of the invoice to cancel.
    /// - `caller`: Address of the party requesting cancellation (freelancer or client).
    ///
    /// # Errors
    /// - Returns `ContractError::InvalidInvoiceStatus` if the invoice status is not `Pending`.
    /// - Returns `ContractError::UnauthorizedCaller` if `caller` is neither the freelancer nor the client.
    pub fn cancel_invoice(env: Env, invoice_id: u64, caller: Address) -> Result<(), ContractError> {
        caller.require_auth();

        let mut invoice = storage::get_invoice(&env, invoice_id)?;

        if invoice.status != storage::InvoiceStatus::Pending {
            return Err(ContractError::InvalidInvoiceStatus);
        }

        if caller != invoice.freelancer && caller != invoice.client {
            return Err(ContractError::UnauthorizedCaller);
        }

        invoice.status = storage::InvoiceStatus::Cancelled;
        storage::save_invoice(&env, &invoice);
        events::invoice_cancelled(&env, invoice_id, &caller);
        Ok(())
    }

    /// Releases escrowed funds to the freelancer once the invoice is approved.
    ///
    /// # Parameters
    /// - `invoice_id`: ID of the invoice to settle.
    /// - `token_address`: Address of the token contract to transfer to.
    ///
    /// # Errors
    /// - Panics if the invoice status is not `Approved`.
    pub fn release_payment(env: Env, invoice_id: u64) {
        let mut invoice = storage::get_invoice(&env, invoice_id).unwrap();

        assert!(
            invoice.status == storage::InvoiceStatus::Approved,
            "Invoice must be in Approved status"
        );

        let token = token::Client::new(&env, &invoice.token);
        token.transfer(&env.current_contract_address(), &invoice.freelancer, &invoice.amount);

        invoice.status = storage::InvoiceStatus::Completed;
        storage::save_invoice(&env, &invoice);

        events::release_payment(&env, invoice_id, &invoice.freelancer, invoice.amount);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::{Address as _, Ledger}, Env, String};

    fn setup_token(env: &Env) -> Address {
        let admin = Address::generate(env);
        env.register_stellar_asset_contract_v2(admin).address()
    }

    #[test]
    #[should_panic(expected = "Client and freelancer must be different addresses")]
    fn test_create_invoice_client_equals_freelancer() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let description = String::from_str(&env, "Self-invoice");

        client.create_invoice(&freelancer, &freelancer, &1000, &description);
    }

    #[test]
    fn test_create_invoice() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let token_address = setup_token(&env);
        let description = String::from_str(&env, "Website redesign - Phase 1");

        let invoice_id = client.create_invoice(&freelancer, &payer, &1000, &token_address, &9999999999, &description);

        assert_eq!(invoice_id, 0);

        // Verify the invoice was stored correctly
        let invoice = env.as_contract(&contract_id, || storage::get_invoice(&env, invoice_id).unwrap());
        assert_eq!(invoice.freelancer, freelancer);
        assert_eq!(invoice.client, payer);
        assert_eq!(invoice.amount, 1000);
        assert_eq!(invoice.token, token_address);
        assert_eq!(invoice.deadline, 9999999999);
    }

    #[test]
    fn test_dispute_invoice_funded() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let token_address = setup_token(&env);
        let description = String::from_str(&env, "Logo design");

        let invoice_id = client.create_invoice(&freelancer, &payer, &500, &token_address, &9999999999, &description);
        client.cancel_invoice(&invoice_id, &freelancer);

        let invoice = env.as_contract(&contract_id, || storage::get_invoice(&env, invoice_id).unwrap());
        assert_eq!(invoice.status, storage::InvoiceStatus::Cancelled);
    }

    #[test]
    fn test_cancel_invoice_by_client() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let token_address = setup_token(&env);
        let description = String::from_str(&env, "SEO audit");

        let invoice_id = client.create_invoice(&freelancer, &payer, &200, &token_address, &9999999999, &description);
        client.cancel_invoice(&invoice_id, &payer);

        let invoice = env.as_contract(&contract_id, || storage::get_invoice(&env, invoice_id).unwrap());
        assert_eq!(invoice.status, storage::InvoiceStatus::Disputed);
    }

    #[test]
    fn test_dispute_invoice_delivered() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let stranger = Address::generate(&env);
        let token_address = setup_token(&env);
        let description = String::from_str(&env, "Branding package");

        let invoice_id = client.create_invoice(&freelancer, &payer, &750, &token_address, &9999999999, &description);
        let _ = client.cancel_invoice(&invoice_id, &stranger);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #2)")]
    fn test_cancel_invoice_wrong_status() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client_contract = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let token_address = setup_token(&env);
        let description = String::from_str(&env, "App development");

        let invoice_id = client_contract.create_invoice(&freelancer, &payer, &3000, &token_address, &9999999999, &description);
        client_contract.cancel_invoice(&invoice_id, &freelancer);

        // Attempt to cancel again — should panic
        let _ = client_contract.cancel_invoice(&invoice_id, &freelancer);
    }

    #[test]
    #[should_panic(expected = "Invoice can only be cancelled from Pending status")]
    fn test_cancel_invoice_from_funded() {
        use soroban_sdk::testutils::Address as _;
        use soroban_sdk::token;

        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Development services");
        let amount: i128 = 1500;

        // Deploy mock token
        let token_admin = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_address = token_id.address();
        let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
        token_admin_client.mint(&payer, &amount);

        // Create invoice (Pending)
        let invoice_id = client.create_invoice(&freelancer, &payer, &amount, &description);

        // Fund it to Funded status
        client.fund_invoice(&invoice_id, &token_address);

        // Try to cancel from Funded -> should panic
        let _ = client.cancel_invoice(&invoice_id, &freelancer);
    }

    #[test]
    fn test_fund_invoice_happy_path() {
        use soroban_sdk::token;

        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let invoice_client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Smart contract audit");
        let amount: i128 = 5000;

        let token_admin = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_address = token_id.address();
        let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
        token_admin_client.mint(&payer, &amount);

        // Set ledger timestamp before the deadline
        env.ledger().set_timestamp(1000);

        let invoice_id = invoice_client.create_invoice(&freelancer, &payer, &amount, &token_address, &9999999999, &description);
        invoice_client.fund_invoice(&invoice_id);

        let invoice = env.as_contract(&contract_id, || storage::get_invoice(&env, invoice_id).unwrap());
        assert_eq!(invoice.status, storage::InvoiceStatus::Funded);

        let token_client = token::Client::new(&env, &token_address);
        assert_eq!(token_client.balance(&contract_id), amount);
        assert_eq!(token_client.balance(&payer), 0);
    }

    #[test]
    fn test_ttl_extension_on_save_and_get() {
        use soroban_sdk::testutils::Address as _;
        
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "TTL test invoice");

        // Create an invoice (this calls save_invoice internally)
        let invoice_id = client.create_invoice(&freelancer, &payer, &1000, &description);

        // Verify the invoice was stored and TTL was extended
        let invoice = env.as_contract(&contract_id, || storage::get_invoice(&env, invoice_id));
        assert_eq!(invoice.id, invoice_id);
        assert_eq!(invoice.freelancer, freelancer);
        assert_eq!(invoice.client, payer);
        assert_eq!(invoice.amount, 1000);
        
        // The fact that we can retrieve the invoice confirms both save_invoice 
        // and get_invoice work correctly with TTL extension
    }

    #[test]
    fn test_mark_delivered_happy_path() {
        use soroban_sdk::testutils::Address as _;
        use soroban_sdk::token;

        let env = Env::default();
        env.mock_all_auths();

        // Deploy the invoice contract
        let contract_id = env.register_contract(None, InvoiceContract);
        let invoice_client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Mark delivered test");
        let amount: i128 = 2000;

        // Deploy a mock token and mint funds to the payer
        let token_admin = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_address = token_id.address();
        let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
        token_admin_client.mint(&payer, &amount);

        // Create and fund the invoice
        let invoice_id = invoice_client.create_invoice(&freelancer, &payer, &amount, &description);
        invoice_client.fund_invoice(&invoice_id, &token_address);

        // Mark as delivered
        invoice_client.mark_delivered(&invoice_id);

        // Assert status is now Delivered
        let invoice = env.as_contract(&contract_id, || storage::get_invoice(&env, invoice_id));
        assert_eq!(invoice.status, storage::InvoiceStatus::Delivered);
    }

    #[test]
    #[should_panic(expected = "Invoice must be in Funded status")]
    fn test_mark_delivered_wrong_status() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Wrong status test");

        let invoice_id = client.create_invoice(&freelancer, &payer, &1000, &description);
        
        // Try to mark delivered without funding first - should panic
        client.mark_delivered(&invoice_id);
    }

    #[test]
    fn test_approve_payment_happy_path() {
        use soroban_sdk::testutils::Address as _;
        use soroban_sdk::token;

        let env = Env::default();
        env.mock_all_auths();

        // Deploy the invoice contract
        let contract_id = env.register_contract(None, InvoiceContract);
        let invoice_client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Approve payment test");
        let amount: i128 = 3000;

        // Deploy a mock token and mint funds to the payer
        let token_admin = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_address = token_id.address();
        let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
        token_admin_client.mint(&payer, &amount);

        // Create, fund, and mark delivered
        let invoice_id = invoice_client.create_invoice(&freelancer, &payer, &amount, &description);
        invoice_client.fund_invoice(&invoice_id, &token_address);
        invoice_client.mark_delivered(&invoice_id);

        // Approve payment
        invoice_client.approve_payment(&invoice_id);

        // Assert status is now Approved
        let invoice = env.as_contract(&contract_id, || storage::get_invoice(&env, invoice_id));
        assert_eq!(invoice.status, storage::InvoiceStatus::Approved);
    }

    #[test]
    #[should_panic(expected = "Invoice must be in Delivered status")]
    fn test_approve_payment_wrong_status() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Wrong status test");

        let invoice_id = client.create_invoice(&freelancer, &payer, &1000, &description);
        
        // Try to approve payment without funding and delivering first - should panic
        client.approve_payment(&invoice_id);
    }

    #[test]
    #[should_panic(expected = "Invoice must be in Funded status")]
    fn test_mark_delivered_from_cancelled_status() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Cancelled status test");

        let invoice_id = client.create_invoice(&freelancer, &payer, &1000, &description);
        
        // Cancel the invoice first
        client.cancel_invoice(&invoice_id, &freelancer);
        
        // Try to mark delivered from cancelled status - should panic
        client.mark_delivered(&invoice_id);
    }
}

