import { Buffer } from 'buffer';
import { PublicKey } from '@solana/web3.js';

export const PROGRAM_ID = new PublicKey('4U2Pgjdq51NXUEDVX4yyFNMdg6PuLHs9ikn9JThkn21p');

// Mayflower
export const MAYFLOWER_PROGRAM_ID = new PublicKey('AVMmmRzwc2kETQNhPiFVnyu62HrgsQXTD6D7SnSfEz7v');
export const MAYFLOWER_TENANT = new PublicKey('81JEJdJSZbaXixpD8WQSBWBfkDa6m6KpXpSErzYUHq6z');
export const MARKET_GROUP = new PublicKey('Lmdgb4NE4T3ubmQZQZQZ7t4UP6A98NdVbmZPcoEdkdC');
export const MARKET_META = new PublicKey('DotD4dZAyr4Kb6AD3RHid8VgmsHUzWF6LRd4WvAMezRj');
export const MAYFLOWER_MARKET = new PublicKey('A5M1nWfi6ATSamEJ1ASr2FC87BMwijthTbNRYG7BhYSc');
export const MARKET_BASE_VAULT = new PublicKey('43vPhZeow3pgYa6zrPXASVQhdXTMfowyfNK87BYizhnL');
export const MARKET_NAV_VAULT = new PublicKey('BCYzijbWwmqRnsTWjGhHbneST2emQY36WcRAkbkhsQMt');
export const FEE_VAULT = new PublicKey('B8jccpiKZjapgfw1ay6EH3pPnxqTmimsm2KsTZ9LSmjf');
export const NAV_SOL_MINT = new PublicKey('navSnrYJkCxMiyhM3F7K889X1u8JFLVHHLxiyo6Jjqo');
export const WSOL_MINT = new PublicKey('So11111111111111111111111111111111111111112');

export const TOKEN_PROGRAM_ID = new PublicKey('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');
export const ATA_PROGRAM_ID = new PublicKey('ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL');
export const SYSTEM_PROGRAM_ID = new PublicKey('11111111111111111111111111111111');

// PDA seeds
const PERSONAL_POSITION_SEED = Buffer.from('personal_position');
const PERSONAL_POSITION_ESCROW_SEED = Buffer.from('personal_position_escrow');
const LOG_SEED = Buffer.from('log');

// PersonalPosition account layout offsets
export const PP_DEPOSITED_SHARES_OFFSET = 104;
export const PP_DEBT_OFFSET = 112;
export const MARKET_FLOOR_PRICE_OFFSET = 104;

// KeyAuthorization
export const KEY_AUTH_SIZE = 74; // 8 + 32 + 32 + 1 + 1

// Derive per-position program PDA (authority)
export function deriveProgramPda(adminNftMint) {
  return PublicKey.findProgramAddressSync(
    [Buffer.from('authority'), adminNftMint.toBuffer()],
    PROGRAM_ID
  );
}

// Derive config PDA
export function deriveConfigPda() {
  return PublicKey.findProgramAddressSync([Buffer.from('config')], PROGRAM_ID);
}

// Derive position PDA
export function derivePositionPda(adminNftMint) {
  return PublicKey.findProgramAddressSync(
    [Buffer.from('position'), adminNftMint.toBuffer()],
    PROGRAM_ID
  );
}

// Derive KeyAuthorization PDA
export function deriveKeyAuthPda(positionPda, keyNftMint) {
  return PublicKey.findProgramAddressSync(
    [Buffer.from('key_auth'), positionPda.toBuffer(), keyNftMint.toBuffer()],
    PROGRAM_ID
  );
}

// Derive Mayflower PersonalPosition PDA
export function derivePersonalPosition(programPda) {
  return PublicKey.findProgramAddressSync(
    [PERSONAL_POSITION_SEED, MARKET_META.toBuffer(), programPda.toBuffer()],
    MAYFLOWER_PROGRAM_ID
  );
}

// Derive Mayflower PersonalPosition escrow
export function derivePersonalPositionEscrow(ppPda) {
  return PublicKey.findProgramAddressSync(
    [PERSONAL_POSITION_ESCROW_SEED, ppPda.toBuffer()],
    MAYFLOWER_PROGRAM_ID
  );
}

// Derive Mayflower log account
export function deriveLogAccount() {
  return PublicKey.findProgramAddressSync(
    [LOG_SEED],
    MAYFLOWER_PROGRAM_ID
  );
}

// Derive ATA
export function getAta(wallet, mint) {
  return PublicKey.findProgramAddressSync(
    [wallet.toBuffer(), TOKEN_PROGRAM_ID.toBuffer(), mint.toBuffer()],
    ATA_PROGRAM_ID
  )[0];
}
