import { AnchorProvider, Program } from '@coral-xyz/anchor';
import idl from './idl.json';
import { PROGRAM_ID } from './constants.js';

export function getProgram(connection, wallet) {
  const provider = new AnchorProvider(connection, wallet, {
    commitment: 'confirmed',
  });
  return new Program(idl, provider);
}
