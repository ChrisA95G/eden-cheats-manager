/** @typedef {'desktop' | 'android'} Platform */

/**
 * @typedef {Object} AppSettings
 * @property {string} apiToken
 * @property {string} pcLoadDir
 * @property {string} prodKeysPath
 * @property {string} packageLibraryPath
 * @property {string} edenExePath
 * @property {boolean} onboardingDone
 */

/**
 * @typedef {Object} EdenLoadAccessStatus
 * @property {boolean} selected
 * @property {boolean} validLocation
 * @property {boolean} readPermission
 * @property {boolean} writePermission
 * @property {boolean} readable
 * @property {boolean} writable
 * @property {boolean} ready
 * @property {string} message
 */

/**
 * @typedef {Object} PackageDiscoveryStatus
 * @property {boolean} prodKeysSelected
 * @property {string} prodKeysName
 * @property {boolean} prodKeysReadable
 * @property {boolean} prodKeysSeekable
 * @property {boolean} packageSelected
 * @property {string} packageName
 * @property {boolean} packageReadable
 * @property {boolean} packageSeekable
 * @property {boolean} ready
 * @property {string} message
 */

/**
 * @typedef {Object} GameLibraryStatus
 * @property {boolean} selected
 * @property {string} name
 * @property {boolean} readPermission
 * @property {boolean} readable
 * @property {boolean} ready
 * @property {string} message
 */

/**
 * @typedef {Object} BootstrapResult
 * @property {Platform} platform
 * @property {AppSettings} settings
 * @property {EdenLoadAccessStatus | null} edenAccess
 * @property {string} edenAccessError
 */

/**
 * @typedef {Object} TitleEntry
 * @property {string} titleId
 * @property {string} baseTitleId
 * @property {string} name
 * @property {string} image
 * @property {'base' | 'update' | 'dlc'} category
 * @property {boolean} installed
 */

/**
 * @typedef {Object} GameGroup
 * @property {string} baseTitleId
 * @property {string} baseName
 * @property {string} baseImage
 * @property {boolean} baseInstalled
 * @property {TitleEntry | null} baseGame
 * @property {TitleEntry[]} updates
 * @property {TitleEntry[]} dlcs
 */

/**
 * @typedef {Object} GameVersionPackage
 * @property {string} contentKind
 * @property {string} titleId
 * @property {string} baseTitleId
 * @property {number} version
 * @property {string} buildId
 * @property {string} moduleId
 * @property {string} packageFormat
 * @property {string} filename
 * @property {string} relativePath
 * @property {number} size
 */

/**
 * @typedef {Object} GameVersionGroup
 * @property {string} baseTitleId
 * @property {GameVersionPackage[]} versions
 */

/**
 * @typedef {Object} GameLibraryScanError
 * @property {string} filename
 * @property {string} relativePath
 * @property {string} message
 */

/**
 * @typedef {Object} EdenPackageCorrelationEntry
 * @property {string} observedTitleId
 * @property {string | null} resolvedBaseTitleId
 * @property {GameVersionPackage[]} packageCandidates
 */

/**
 * @typedef {Object} EdenPackageCorrelationIssue
 * @property {string} observedTitleId
 * @property {string} message
 */

/**
 * @typedef {Object} EdenPackageCorrelationResult
 * @property {number} scannedPackages
 * @property {number} matchedPackages
 * @property {number} skippedPackages
 * @property {EdenPackageCorrelationEntry[]} edenEntries
 * @property {GameVersionGroup[]} unmatchedPackageGroups
 * @property {GameLibraryScanError[]} packageScanErrors
 * @property {EdenPackageCorrelationIssue[]} correlationIssues
 */

/**
 * @typedef {{ state: 'notConfigured', message: string }
 *   | { state: 'ready', correlation: EdenPackageCorrelationResult }
 *   | { state: 'error', message: string }} ManagedPackageLibrary
 */

/**
 * @typedef {Object} ManagedLibrarySnapshot
 * @property {GameGroup[]} games
 * @property {ManagedPackageLibrary} packageLibrary
 */

/**
 * @typedef {Object} PackageMetadata
 * @property {string} packageFormat
 * @property {string} contentKind
 * @property {string} titleId
 * @property {string} baseTitleId
 * @property {string} programTitleId
 * @property {number} version
 * @property {string} buildId
 * @property {string} moduleId
 * @property {boolean} hasBktr
 * @property {boolean} matchedProgramContentId
 */

/**
 * @typedef {Object} InstalledCheat
 * @property {string} cheatName
 * @property {string} buildId
 */

/**
 * @typedef {Object} CheatEntry
 * @property {number} id
 * @property {string} buildId
 * @property {string} content
 * @property {string} credits
 * @property {string} description
 * @property {boolean} custom
 */

/**
 * @typedef {Object} GameInfo
 * @property {string} slug
 * @property {string} name
 * @property {string} image
 * @property {string} titleId
 * @property {CheatEntry[]} cheats
 */

export {};
