// Enforces that conversion crates remain independent of Tauri as defined in
// design §3 ("ディレクトリ構成", crates/core is Tauri-independent). Neither
// pdfconv-core nor pdfconv-worker may depend on tauri, tauri-*, wry, or tao
// (directly or transitively via runtime or build dependencies).
//
// Usage: node scripts/check-layers.ts

import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT_PACKAGES: readonly string[] = ["pdfconv-core", "pdfconv-worker"];
const FORBIDDEN_EXACT_NAMES: readonly string[] = ["tauri", "wry", "tao"];
const FORBIDDEN_PREFIX = "tauri-";
const BUILD_DEP_KIND = "build";
const MAX_METADATA_BUFFER_BYTES = 64 * 1024 * 1024;

export type CargoPackage = {
  readonly id: string;
  readonly name: string;
};

export type CargoDepKind = {
  readonly kind: string | null;
  readonly target?: string | null;
};

export type CargoNodeDep = {
  readonly name: string;
  readonly pkg: string;
  readonly dep_kinds?: readonly CargoDepKind[];
};

export type CargoNode = {
  readonly id: string;
  readonly deps: readonly CargoNodeDep[];
};

export type CargoMetadata = {
  readonly packages: readonly CargoPackage[];
  readonly resolve?: {
    readonly nodes: readonly CargoNode[];
  } | null;
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isPackage(value: unknown): value is CargoPackage {
  return (
    isRecord(value) &&
    typeof value.id === "string" &&
    typeof value.name === "string"
  );
}

function isDepKind(value: unknown): value is CargoDepKind {
  return (
    isRecord(value) && (value.kind === null || typeof value.kind === "string")
  );
}

function isNodeDep(value: unknown): value is CargoNodeDep {
  if (
    !isRecord(value) ||
    typeof value.name !== "string" ||
    typeof value.pkg !== "string"
  ) {
    return false;
  }
  if (value.dep_kinds === undefined) {
    return true;
  }
  return Array.isArray(value.dep_kinds) && value.dep_kinds.every(isDepKind);
}

function isNode(value: unknown): value is CargoNode {
  return (
    isRecord(value) &&
    typeof value.id === "string" &&
    Array.isArray(value.deps) &&
    value.deps.every(isNodeDep)
  );
}

function isCargoMetadata(value: unknown): value is CargoMetadata {
  if (
    !isRecord(value) ||
    !Array.isArray(value.packages) ||
    !value.packages.every(isPackage)
  ) {
    return false;
  }
  if (value.resolve === undefined || value.resolve === null) {
    return true;
  }
  if (!isRecord(value.resolve) || !Array.isArray(value.resolve.nodes)) {
    return false;
  }
  return value.resolve.nodes.every(isNode);
}

function isForbidden(packageName: string): boolean {
  return (
    FORBIDDEN_EXACT_NAMES.includes(packageName) ||
    packageName.startsWith(FORBIDDEN_PREFIX)
  );
}

function isFollowableDep(dep: CargoNodeDep): boolean {
  if (dep.dep_kinds === undefined || dep.dep_kinds.length === 0) {
    return true;
  }
  return dep.dep_kinds.some(
    (dk) => dk.kind === null || dk.kind === BUILD_DEP_KIND,
  );
}

type QueueEntry = {
  readonly id: string;
  readonly path: readonly string[];
};

export function checkLayers(metadata: unknown): string[] {
  if (!isCargoMetadata(metadata)) {
    throw new Error("Invalid cargo metadata structure");
  }
  if (!metadata.resolve) {
    throw new Error(
      "Cargo metadata has no resolve graph (was --no-deps used?)",
    );
  }

  const packageById = new Map<string, CargoPackage>();
  for (const pkg of metadata.packages) {
    packageById.set(pkg.id, pkg);
  }

  const nodeById = new Map<string, CargoNode>();
  for (const node of metadata.resolve.nodes) {
    nodeById.set(node.id, node);
  }

  const violations: string[] = [];

  for (const rootName of ROOT_PACKAGES) {
    const rootPkg = metadata.packages.find((pkg) => pkg.name === rootName);
    if (!rootPkg) {
      throw new Error(
        `Root package "${rootName}" was not found in cargo metadata packages`,
      );
    }

    const rootNode = nodeById.get(rootPkg.id);
    if (!rootNode) {
      throw new Error(
        `Root node for "${rootName}" (${rootPkg.id}) was not found in cargo metadata resolve graph`,
      );
    }

    const queue: QueueEntry[] = [{ id: rootPkg.id, path: [rootPkg.name] }];
    const visited = new Set<string>([rootPkg.id]);

    while (queue.length > 0) {
      const current = queue.shift();
      if (!current) {
        break;
      }

      const node = nodeById.get(current.id);
      if (!node) {
        continue;
      }

      for (const dep of node.deps) {
        if (!isFollowableDep(dep)) {
          continue;
        }

        const depPkg = packageById.get(dep.pkg);
        if (!depPkg) {
          continue;
        }

        const nextPath = [...current.path, depPkg.name];
        if (isForbidden(depPkg.name)) {
          const pathStr = nextPath.join(" -> ");
          if (!violations.includes(pathStr)) {
            violations.push(pathStr);
          }
          continue;
        }

        if (!visited.has(depPkg.id)) {
          visited.add(depPkg.id);
          queue.push({ id: depPkg.id, path: nextPath });
        }
      }
    }
  }

  return violations;
}

function fetchCargoMetadata(): unknown {
  const stdout = execFileSync(
    "cargo",
    ["metadata", "--format-version", "1", "--locked"],
    {
      encoding: "utf8",
      maxBuffer: MAX_METADATA_BUFFER_BYTES,
    },
  );
  const parsed: unknown = JSON.parse(stdout);
  return parsed;
}

function main(): void {
  const metadata = fetchCargoMetadata();
  const violations = checkLayers(metadata);
  if (violations.length > 0) {
    for (const line of violations) {
      console.error(line);
    }
    process.exit(1);
  }
}

if (
  process.argv[1] !== undefined &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  try {
    main();
  } catch (error: unknown) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
