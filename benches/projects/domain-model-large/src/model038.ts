// Generated domain module 38 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model037Service, summarizeModel037 } from "./model037";
import type { Model037 } from "./model037";
import { Model031Service, summarizeModel031 } from "./model031";
import type { Model031 } from "./model031";

export type Model038Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model038Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model038 extends Entity<"model038"> {
  updatedAt?: number;
  name: string;
  status: Model038Status;
  tags: string[];
  lines: Model038Line[];
  owner?: { name: string; email?: string };
  parent?: Model037;
  related: Model031[];
}

export type Model038Event =
  | { kind: "created"; item: Model038 }
  | { kind: "renamed"; id: Id<"model038">; from: string; to: string }
  | { kind: "moved"; id: Id<"model038">; status: Model038Status }
  | { kind: "deleted"; id: Id<"model038">; reason?: string };

export type Model038Events = {
  change: Model038Event;
  error: { message: string; code: number };
};

export type Model038Numbers = KeysOfType<Model038Line, number>;
export type FrozenModel038 = DeepReadonly<Model038>;

export function describeModel038Event(event: Model038Event): string {
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

export function totalModel038(item: Model038): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel038(item: Model038): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel038(item), 3), (item.parent ? summarizeModel037(item.parent) : "-"), item.related.map(summarizeModel031).join(",")].join(" | ");
}

export class Model038Service extends Service<Model038, "model038"> {
  readonly events = new EventBus<Model038Events>();
  private readonly parents?: Model037Service;

  constructor(repository = new MemoryRepository<Model038, "model038">()) {
    super(repository);
  }

  validate(item: Model038): string[] {
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

  rename(id: Id<"model038">, to: string): Result<Model038> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model038 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model038">, status: Model038Status): Result<Model038Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model038">, patch: Patch<Pick<Model038, "name" | "tags">>): Model038 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model038Status, Model038[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model038Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model038">[]): Promise<Model038[]> {
    const found: Model038[] = [];
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

  linkedService(): Model037Service {
    return this.parents ?? new Model037Service();
  }
}

export function makeModel038(id: string, name: string): Model038 {
  return {
    id: id as Id<"model038">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 39, unit: "item" }],
    related: [],
  };
}

export const model038Defaults: FrozenModel038 = makeModel038("default-38", "Default 38");
export const model038Label = summarizeModel038(makeModel038("label", "Label"));
