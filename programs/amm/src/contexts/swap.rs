use anchor_lang::{accounts::account, prelude::*, solana_program::address_lookup_table::instruction};
use constant_product_curve::{ConstantProduct, LiquidityPair};
use crate::state::config::Config;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked, mint_to, MintTo},
};
use crate::error::AmmError;
use crate::{assert_non_zero,assert_not_expired,assert_not_locked};

#[derive(Accounts)]
pub struct Swap<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    pub mint_x: Box<InterfaceAccount<'info, Mint>>,
    pub mint_y: Box<InterfaceAccount<'info, Mint>>,
    #[account(
        mut,
        seeds = [b"lp", config.key().as_ref()],
        bump = config.lp_bump,
        mint::decimals = 6,
        mint::authority = auth
    )]
    pub mint_lp: Box<InterfaceAccount<'info, Mint>>,
    #[account(
        mut, 
        associated_token::mint = mint_x,
        associated_token::authority = auth
    )]
    pub vault_x: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        mut, 
        associated_token::mint = mint_y,
        associated_token::authority = auth
    )]
    pub vault_y: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = mint_x,
        associated_token::authority = user,
    )]
    pub user_x: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = mint_y,
        associated_token::authority = user,
    )]
    pub user_y: Box<InterfaceAccount<'info, TokenAccount>>,
    /// CHECK: this is safe
    #[account(
        seeds = [b"auth"],
        bump = config.auth_bump,
    )]
    pub auth: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [b"config", config.seed.to_le_bytes().as_ref()],
        bump = config.config_bump,
    )]
    pub config: Account<'info, Config>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

impl<'info> Swap<'info> {

    pub fn swap(
        &mut self,
        is_x: bool,
        amount:u64,
        min: u64,
        expiration: i64,
    ) -> Result<()> {
        assert_not_locked!(self.config.locked);
        assert_not_expired!(expiration);
        assert_non_zero!([amount]);

        let mut curve = ConstantProduct::init(self.vault_x.amount, self.vault_y.amount, self.mint_lp.supply, self.config.fee, None).map_err(AmmError::from)?;
        
        let p = match is_x {
            true => LiquidityPair::X,
            false => LiquidityPair::Y,
        };

        let res = curve.swap(p, amount, min).map_err(AmmError::from)?;

        assert_non_zero!([res.deposit, res.withdraw]);

        self.deposit_token(is_x, res.deposit)?;
        self.withdraw_token(is_x, res.withdraw)?;
        Ok(())
    }
    pub fn deposit_token(
        &mut self,
        is_x: bool,
        amount:u64,
    ) -> Result<()> {
        let mint;
        let (from, to) = match is_x {
            true => {
                mint = self.mint_x.clone();
                (self.user_x.to_account_info(), self.vault_x.to_account_info())
            },
            false => {
                mint = self.mint_y.clone();
                (self.user_y.to_account_info(), self.vault_y.to_account_info())
            }
        };

        let account = TransferChecked {
            from,
            mint: mint.to_account_info(),
            to,
            authority: self.user.to_account_info()
        };
        let ctx = CpiContext::new(self.token_program.to_account_info(), account);

        transfer_checked(ctx, amount, 6)
    }
    pub fn withdraw_token(
        &mut self,
        is_x: bool,
        amount:u64
    ) -> Result<()> {
        let mint;
        let (from, to) = match is_x {
            true => {
                mint = self.mint_x.clone();
                (self.vault_y.to_account_info(), self.user_y.to_account_info())
            },
            false => {
                mint = self.mint_y.clone();
                (self.vault_x.to_account_info(), self.user_x.to_account_info())
            }
        };

        let account = TransferChecked{
            from,
            mint: mint.to_account_info(),
            to,
            authority: self.auth.to_account_info()
        };

        let seeds = &[
            &b"auth"[..],
            &[self.config.auth_bump],
        ];
        let signer_seeds = &[&seeds[..]];

        let ctx = CpiContext::new_with_signer(self.token_program.to_account_info(), account, signer_seeds);
        transfer_checked(ctx, amount, 6)

    }
}