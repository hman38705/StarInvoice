#![no_std]

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
            description,
            status: storage::InvoiceStatus::Pending,
        };

        storage::save_invoice(&env, &invoice);
        events::invoice_created(&env, invoice_id, &freelancer, &client, amount);

        invoice_id
    }

    /// Allows the client to deposit funds into escrow for the given invoice.
    ///
    /// # Parameters
    /// - `invoice_id`: ID of the invoice to fund.
    ///
    /// # Errors
    /// - Panics if the caller is not the invoice client.
/// - Returns `ContractError::InvalidInvoiceStatus` if the invoice status is not `Pending`.
    pub fn fund_invoice(env: Env, invoice_id: u64, token_address: Address) -> Result<(), ContractError> {
        let mut invoice = storage::get_invoice(&env, invoice_id)?;

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
        Ok(())
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

        if invoice.status != storage::InvoiceStatus::Delivered {
            return Err(ContractError::InvalidInvoiceStatus);
        }

        invoice.status = storage::InvoiceStatus::Approved;
        storage::save_invoice(&env, &invoice);

        events::approve_payment(&env, invoice_id, &invoice.client);
        Ok(())
    }

    /// Returns the current number of invoices.
    pub fn invoice_count(env: Env) -> u64 {
        storage::get_invoice_count(&env)
    }

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
        let mut invoice = storage::get_invoice(&env, invoice_id);

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
    fn test_cancel_invoice_by_freelancer() {
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
        assert_eq!(invoice.status, storage::InvoiceStatus::Cancelled);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #3)")]
    fn test_cancel_invoice_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let stranger = Address::generate(&env);
        let token_address = setup_token(&env);
        let description = String::from_str(&env, "Branding package");

        let invoice_id = client.create_invoice(&freelancer, &payer, &750, &description);
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

        // Assert status is now Funded
        let invoice = env.as_contract(&contract_id, || storage::get_invoice(&env, invoice_id).unwrap());
        assert_eq!(invoice.status, storage::InvoiceStatus::Funded);

        let token_client = token::Client::new(&env, &token_address);
        assert_eq!(token_client.balance(&contract_id), amount);
        assert_eq!(token_client.balance(&payer), 0);
    }
    #[test]
    fn test_invoice_count() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        assert_eq!(client.invoice_count(), 0);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);

        client.create_invoice(
            &freelancer,
            &payer,
            &1000,
            &String::from_str(&env, "Desc 1"),
        );
        assert_eq!(client.invoice_count(), 1);

        client.create_invoice(
            &freelancer,
            &payer,
            &2000,
            &String::from_str(&env, "Desc 2"),
        );
        assert_eq!(client.invoice_count(), 2);
    }

    #[test]
    fn test_get_invoice() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Test get_invoice");

        let invoice_id = client.create_invoice(&freelancer, &payer, &1000, &description);
        let invoice = client.get_invoice(&invoice_id);

        assert_eq!(invoice.id, invoice_id);
        assert_eq!(invoice.freelancer, freelancer);
        assert_eq!(invoice.client, payer);
        assert_eq!(invoice.amount, 1000);
        assert_eq!(invoice.description, description);
    }

    #[test]
    fn test_invoice_not_found() {
        let env = Env::default();
        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let result = client.try_get_invoice(&999);
        match result {
            Err(Ok(errors)) => {
                assert_eq!(errors, ContractError::InvoiceNotFound.into());
            }
            _ => panic!("Expected InvoiceNotFound error"),
        }
    }

    // Issue #80: Negative tests for wrong-caller authorization
    #[test]
    #[should_panic]
    fn test_fund_invoice_wrong_caller() {
        let env = Env::default();
        // Do not mock all auths to test auth failure

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Test funding");

        let invoice_id = client.create_invoice(&freelancer, &payer, &1000, &description);

        // Try to fund as freelancer (wrong caller) - should panic
        let token_address = Address::generate(&env); // dummy
        let _ = client.fund_invoice(&invoice_id, &token_address);
    }

    #[test]
    #[should_panic]
    fn test_mark_delivered_wrong_caller() {
        let env = Env::default();
        // Do not mock all auths

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Test delivery");

        let invoice_id = client.create_invoice(&freelancer, &payer, &1000, &description);

        // Fund the invoice first
        env.mock_all_auths(); // temporarily mock to fund
        let token_admin = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_address = token_id.address();
        let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
        token_admin_client.mint(&payer, &1000);
        client.fund_invoice(&invoice_id, &token_address);
        env.set_auths(&[]); // clear mocks

        // Try to mark delivered as client (wrong caller) - should panic
        let _ = client.mark_delivered(&invoice_id);
    }

    #[test]
    #[should_panic]
    fn test_approve_payment_wrong_caller() {
        let env = Env::default();
        // Do not mock all auths

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Test approval");

        let invoice_id = client.create_invoice(&freelancer, &payer, &1000, &description);

        // Fund and deliver the invoice first
        env.mock_all_auths();
        let token_admin = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_address = token_id.address();
        let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
        token_admin_client.mint(&payer, &1000);
        client.fund_invoice(&invoice_id, &token_address);
        client.mark_delivered(&invoice_id);
        env.set_auths(&[]);

        // Try to approve as freelancer (wrong caller) - should panic
        let _ = client.approve_payment(&invoice_id);
    }

    // Issue #81: Tests for invalid status transitions
    #[test]
    #[should_panic(expected = "Invoice must be in Pending status")]
    fn test_fund_invoice_already_funded() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Test double funding");

        let invoice_id = client.create_invoice(&freelancer, &payer, &1000, &description);

        let token_admin = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_address = token_id.address();
        let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
        token_admin_client.mint(&payer, &2000); // mint extra

        client.fund_invoice(&invoice_id, &token_address);
        // Try to fund again - should panic
        let _ = client.fund_invoice(&invoice_id, &token_address);
    }

    #[test]
    #[should_panic(expected = "Invoice must be in Funded status")]
    fn test_mark_delivered_pending_invoice() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Test mark delivered on pending");

        let invoice_id = client.create_invoice(&freelancer, &payer, &1000, &description);

        // Try to mark delivered on pending - should panic
        let _ = client.mark_delivered(&invoice_id);
    }

    #[test]
    #[should_panic(expected = "Invoice must be in Delivered status")]
    fn test_approve_payment_funded_invoice() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Test approve on funded");

        let invoice_id = client.create_invoice(&freelancer, &payer, &1000, &description);

        let token_admin = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_address = token_id.address();
        let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
        token_admin_client.mint(&payer, &1000);
        client.fund_invoice(&invoice_id, &token_address);

        // Try to approve on funded (not delivered) - should panic
        let _ = client.approve_payment(&invoice_id);
    }

    #[test]
    #[should_panic(expected = "Invoice must be in Approved status")]
    fn test_release_payment_delivered_invoice() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Test release on delivered");

        let invoice_id = client.create_invoice(&freelancer, &payer, &1000, &description);

        let token_admin = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_address = token_id.address();
        let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
        token_admin_client.mint(&payer, &1000);
        client.fund_invoice(&invoice_id, &token_address);
        client.mark_delivered(&invoice_id);

        // Try to release on delivered (not approved) - should panic
        let _ = client.release_payment(&invoice_id, &token_address);
    }

    // Issue #82: End-to-end test covering the full escrow flow
    #[test]
    fn test_full_escrow_flow() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Full escrow flow test");
        let amount: i128 = 5000;

        // Deploy token and mint to payer
        let token_admin = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_address = token_id.address();
        let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
        token_admin_client.mint(&payer, &amount);

        let token_client = token::Client::new(&env, &token_address);

        // Step 1: Create invoice
        let invoice_id = client.create_invoice(&freelancer, &payer, &amount, &description);
        let invoice = env.as_contract(&contract_id, || storage::get_invoice(&env, invoice_id).unwrap());
        assert_eq!(invoice.status, storage::InvoiceStatus::Pending);

        // Step 2: Fund invoice
        client.fund_invoice(&invoice_id, &token_address);
        let invoice = env.as_contract(&contract_id, || storage::get_invoice(&env, invoice_id).unwrap());
        assert_eq!(invoice.status, storage::InvoiceStatus::Funded);
        assert_eq!(token_client.balance(&contract_id), amount);
        assert_eq!(token_client.balance(&payer), 0);

        // Step 3: Mark delivered
        client.mark_delivered(&invoice_id);
        let invoice = env.as_contract(&contract_id, || storage::get_invoice(&env, invoice_id).unwrap());
        assert_eq!(invoice.status, storage::InvoiceStatus::Delivered);

        // Step 4: Approve payment
        client.approve_payment(&invoice_id);
        let invoice = env.as_contract(&contract_id, || storage::get_invoice(&env, invoice_id).unwrap());
        assert_eq!(invoice.status, storage::InvoiceStatus::Approved);

        // Step 5: Release payment
        client.release_payment(&invoice_id, &token_address);
        let invoice = env.as_contract(&contract_id, || storage::get_invoice(&env, invoice_id).unwrap());
        assert_eq!(invoice.status, storage::InvoiceStatus::Completed);

        // Assert final balances
        assert_eq!(token_client.balance(&contract_id), 0);
        assert_eq!(token_client.balance(&freelancer), amount);
        assert_eq!(token_client.balance(&payer), 0);
    }

    // Issue #83: Test for create_invoice with duplicate IDs (regression)
    #[test]
    fn test_create_invoice_unique_ids() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, InvoiceContract);
        let client = InvoiceContractClient::new(&env, &contract_id);

        let freelancer = Address::generate(&env);
        let payer = Address::generate(&env);
        let description = String::from_str(&env, "Unique ID test");

        let mut ids: soroban_sdk::Vec<u64> = soroban_sdk::Vec::new(&env);

        for i in 0..10u64 {
            let invoice_id = client.create_invoice(&freelancer, &payer, &1000, &description);
            assert_eq!(invoice_id, i);
            // Check not already in ids
            let mut is_unique = true;
            for existing_id in ids.iter() {
                if existing_id == invoice_id {
                    is_unique = false;
                    break;
                }
            }
            assert!(is_unique, "Duplicate ID found: {}", invoice_id);
            ids.push_back(invoice_id);
        }

        assert_eq!(client.invoice_count(), 10);
    }
}
