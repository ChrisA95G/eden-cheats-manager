/** @typedef {import('../api/types.js').EdenLoadAccessStatus} FolderStatus */

/**
 * The picker, mode choice and permission save all belong to the same selection.
 * Only publish the result once Android has finished all three.
 * @param {() => Promise<FolderStatus>} readStatus
 * @param {() => boolean} isCurrent
 */
export async function readSettledInstallFolder(readStatus, isCurrent) {
  while (isCurrent()) {
    const status = await readStatus();
    if (!isCurrent()) break;
    if (!status.selectionPending) return status;
    await new Promise(resolve => setTimeout(resolve, 300));
  }
  return null;
}

/** @param {FolderStatus | null} previous @param {FolderStatus} next */
export function installFolderChanged(previous, next) {
  return previous?.destinationId !== next.destinationId || previous?.installMode !== next.installMode;
}
