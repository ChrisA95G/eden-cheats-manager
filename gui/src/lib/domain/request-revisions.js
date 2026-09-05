/**
 * Create selection-scoped request tokens. A resource revision also prevents an
 * older same-selection read from overwriting a newer post-mutation refresh.
 *
 * Tokens belong to this one manager instance and must not cross workspaces.
 *
 * @param {string[]} resources
 */
export function createRequestRevisions(resources) {
  let selectionRevision = 0;
  const revisions = new Map(resources.map((resource) => [resource, 0]));

  function beginSelection() {
    selectionRevision += 1;
    for (const resource of revisions.keys()) revisions.set(resource, 0);
    return selectionRevision;
  }

  /** @param {string} resource */
  function begin(resource) {
    if (!revisions.has(resource)) {
      throw new Error(`Unknown request resource: ${resource}`);
    }
    const resourceRevision = (revisions.get(resource) ?? 0) + 1;
    revisions.set(resource, resourceRevision);
    return { selectionRevision, resource, resourceRevision };
  }

  /** @param {{ selectionRevision: number, resource: string, resourceRevision: number }} token */
  function isCurrent(token) {
    return token.selectionRevision === selectionRevision
      && revisions.get(token.resource) === token.resourceRevision;
  }

  /** @param {string} resource */
  function invalidate(resource) {
    begin(resource);
  }

  return { beginSelection, begin, isCurrent, invalidate };
}
