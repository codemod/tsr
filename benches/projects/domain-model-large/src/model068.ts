// Generated domain module 68 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model067Service, summarizeModel067 } from "./model067";
import type { Model067 } from "./model067";
import { Model061Service, summarizeModel061 } from "./model061";
import type { Model061 } from "./model061";

export type Model068Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model068Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model068 extends Entity<"model068"> {
  updatedAt?: number;
  name: string;
  status: Model068Status;
  tags: string[];
  lines: Model068Line[];
  owner?: { name: string; email?: string };
  parent?: Model067;
  related: Model061[];
}

export type Model068Event =
  | { kind: "created"; item: Model068 }
  | { kind: "renamed"; id: Id<"model068">; from: string; to: string }
  | { kind: "moved"; id: Id<"model068">; status: Model068Status }
  | { kind: "deleted"; id: Id<"model068">; reason?: string };

export type Model068Events = {
  change: Model068Event;
  error: { message: string; code: number };
};

export type Model068Numbers = KeysOfType<Model068Line, number>;
export type FrozenModel068 = DeepReadonly<Model068>;

export function describeModel068Event(event: Model068Event): string {
  switch (event.kind) {
    case "created":
      return `created ${event.item.name}`;
    case "renamed":
      return `renamed ${event.from} -> ${event.to}`;
    case "moved":
      return `moved to ${event.status}`;
    case "deleted":
      return event.reason ? `deleted: ${event.reason}` : "deleted";
    default: {
      const unreachable: never = event;
      return unreachable;
    }
  }
}

export function totalModel068(item: Model068): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel068(item: Model068): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel068(item), 3), (item.parent ? summarizeModel067(item.parent) : "-"), item.related.map(summarizeModel061).join(",")].join(" | ");
}

export class Model068Service extends Service<Model068, "model068"> {
  readonly events = new EventBus<Model068Events>();
  private readonly parents?: Model067Service;

  constructor(repository = new MemoryRepository<Model068, "model068">()) {
    super(repository);
  }

  validate(item: Model068): string[] {
    const problems: string[] = [];
    if (item.name.trim().length === 0) {
      problems.push("name is required");
    }
    for (const [index, line] of item.lines.entries()) {
      if (line.quantity <= 0) {
        problems.push(`line ${index} has no quantity`);
      }
    }
    return problems;
  }

  rename(id: Id<"model068">, to: string): Result<Model068> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model068 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model068">, status: Model068Status): Result<Model068Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model068">, patch: Patch<Pick<Model068, "name" | "tags">>): Model068 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model068Status, Model068[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model068Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model068">[]): Promise<Model068[]> {
    const found: Model068[] = [];
    for (const id of ids) {
      const item = await retry(3, async () => this.find((candidate) => candidate.id === id));
      if (item) {
        found.push(item);
      }
    }
    return found;
  }

  parentNames(): string[] {
    return this.repository
      .list((item) => item.parent !== undefined)
      .map((item) => unwrapOr(ok(item.parent?.name ?? ""), ""));
  }

  linkedService(): Model067Service {
    return this.parents ?? new Model067Service();
  }
}

export function makeModel068(id: string, name: string): Model068 {
  return {
    id: id as Id<"model068">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 69, unit: "item" }],
    related: [],
  };
}

export const model068Defaults: FrozenModel068 = makeModel068("default-68", "Default 68");
export const model068Label = summarizeModel068(makeModel068("label", "Label"));
