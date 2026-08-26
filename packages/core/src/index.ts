export * from './errors.js';
export * from './repository.js';
export * from './permissions.js';
export * from './contact-logic.js';
export * from './import.js';
export * from './export.js';
export * from './mock-repository.js';

// `contract-tests` is deliberately NOT re-exported here. It imports `vitest`,
// and anything reachable from this entry point ends up in an application
// bundle: a production build tree-shakes it away and looks fine, while a dev
// server serves the module as written and the browser dies on the vitest
// import. Test helpers are reached through the `@yanuka/core/testing` subpath
// instead, which application code never imports.
