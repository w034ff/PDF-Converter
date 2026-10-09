import type { ErrorCode, IpcError } from "../../ipc";

/** The codes of a conversion refused because of its output folder (design §6.5). */
const OUTPUT_DIR_ERROR_CODES: readonly ErrorCode[] = [
  "OutputDirMissing",
  "OutputDirNotWritable",
];

/** Whether `error` says the output folder cannot be used. */
export function isOutputDirError(error: IpcError): boolean {
  return OUTPUT_DIR_ERROR_CODES.includes(error.code);
}
