// Generated domain module 44 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model043Service, summarizeModel043 } from "./model043";
import type { Model043 } from "./model043";
import { Model037Service, summarizeModel037 } from "./model037";
import type { Model037 } from "./model037";

export type Model044Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model044Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model044 extends Entity<"model044"> {
  updatedAt?: number;
  name: string;
  status: Model044Status;
  tags: string[];
  lines: Model044Line[];
  owner?: { name: string; email?: string };
  parent?: Model043;
  related: Model037[];
}

export type Model044Event =
  | { kind: "created"; item: Model044 }
  | { kind: "renamed"; id: Id<"model044">; from: string; to: string }
  | { kind: "moved"; id: Id<"model044">; status: Model044Status }
  | { kind: "deleted"; id: Id<"model044">; reason?: string };

export type Model044Events = {
  change: Model044Event;
  error: { message: string; code: number };
};

export type Model044Numbers = KeysOfType<Model044Line, number>;
export type FrozenModel044 = DeepReadonly<Model044>;

export function describeModel044Event(event: Model044Event): string {
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

export function totalModel044(item: Model044): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel044(item: Model044): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel044(item), 3), (item.parent ? summarizeModel043(item.parent) : "-"), item.related.map(summarizeModel037).join(",")].join(" | ");
}

export class Model044Service extends Service<Model044, "model044"> {
  readonly events = new EventBus<Model044Events>();
  private readonly parents?: Model043Service;

  constructor(repository = new MemoryRepository<Model044, "model044">()) {
    super(repository);
  }

  validate(item: Model044): string[] {
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

  rename(id: Id<"model044">, to: string): Result<Model044> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model044 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model044">, status: Model044Status): Result<Model044Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model044">, patch: Patch<Pick<Model044, "name" | "tags">>): Model044 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model044Status, Model044[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model044Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model044">[]): Promise<Model044[]> {
    const found: Model044[] = [];
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

  linkedService(): Model043Service {
    return this.parents ?? new Model043Service();
  }
}

export function makeModel044(id: string, name: string): Model044 {
  return {
    id: id as Id<"model044">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 45, unit: "hour" }],
    related: [],
  };
}

export const model044Defaults: FrozenModel044 = makeModel044("default-44", "Default 44");
export const model044Label = summarizeModel044(makeModel044("label", "Label"));
