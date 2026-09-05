/** @typedef {import('../api/types.js').CheatEntry} CheatEntry */
/** @typedef {import('../api/types.js').InstalledCheat} InstalledCheat */

/**
 * Split one cheat file into its named sections using released parsing behavior.
 * Text before the first bracketed header and trailing whitespace are ignored.
 *
 * @param {string} content
 * @returns {{ name: string, content: string }[]}
 */
export function parseCheatSections(content) {
  /** @type {{ name: string, lines: string[] }[]} */
  const sections = [];
  /** @type {{ name: string, lines: string[] } | null} */
  let current = null;

  for (const line of content.split('\n')) {
    const trimmed = line.trim();
    if (trimmed.startsWith('[') && trimmed.endsWith(']') && trimmed.length > 2) {
      if (current) sections.push(current);
      current = {
        name: trimmed.slice(1, -1),
        lines: [line],
      };
    } else if (current) {
      current.lines.push(line);
    }
  }

  if (current) sections.push(current);
  return sections.map((section) => ({
    name: section.name,
    content: section.lines.join('\n').trimEnd(),
  }));
}

/**
 * Keep this transformation in sync with the backend filesystem contract.
 * Cropping happens before filtering to preserve released behavior.
 *
 * @param {string} sectionName
 */
export function toCheatName(sectionName) {
  return sectionName.slice(0, 60).replace(/[^\w\s\-()]/g, '').trim();
}

/** @param {string} sectionName @param {string} buildId */
export function cheatFileName(sectionName, buildId) {
  return toCheatName(sectionName) || `cheat_${buildId}`;
}

/**
 * @typedef {Object} CheatSection
 * @property {string} name
 * @property {string} content
 * @property {number} entryId
 * @property {number} sectionIndex
 * @property {boolean} custom
 * @property {string} credits
 */

/**
 * @typedef {Object} CheatBuildGroup
 * @property {string} buildId
 * @property {string} credits
 * @property {CheatSection[]} sections
 * @property {{ entryId: number, content: string }[]} customEntries
 */

/**
 * Group catalog entries without losing source-row identity or section order.
 *
 * @param {CheatEntry[]} entries
 * @returns {CheatBuildGroup[]}
 */
export function groupCheatEntries(entries) {
  /** @type {Map<string, CheatBuildGroup>} */
  const groups = new Map();

  for (const entry of entries) {
    const buildId = entry.buildId.toUpperCase();
    let group = groups.get(buildId);
    if (!group) {
      group = { buildId, credits: '', sections: [], customEntries: [] };
      groups.set(buildId, group);
    }

    if (!entry.custom && !group.credits && entry.credits.trim()) {
      group.credits = entry.credits;
    }
    if (entry.custom) {
      group.customEntries.push({ entryId: entry.id, content: entry.content });
    }

    parseCheatSections(entry.content).forEach((section, sectionIndex) => {
      group.sections.push({
        ...section,
        entryId: entry.id,
        sectionIndex,
        custom: entry.custom,
        credits: entry.credits,
      });
    });
  }

  return [...groups.values()];
}

/** @param {string} buildId @param {string} cheatName */
export function installedTupleKey(buildId, cheatName) {
  return JSON.stringify([buildId.toUpperCase(), cheatName]);
}

/** @param {InstalledCheat[]} installed */
export function createInstalledIndex(installed) {
  return new Set(installed.map((item) => installedTupleKey(item.buildId, item.cheatName)));
}

/** @param {number} entryId @param {number} sectionIndex */
export function sectionActionKey(entryId, sectionIndex) {
  return JSON.stringify([entryId, sectionIndex]);
}
