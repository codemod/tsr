export type Box<T> = { value: T; previous?: Box<T> };
export type Row<T> = { [K in keyof T]: Box<T[K]> };
export type Value<T> = T extends Box<infer V> ? V : never;
export type Unbox<T> = { [K in keyof T]: Value<T[K]> };

export interface Item {
  id: string;
  count: number;
  active: boolean;
}

export function box<T>(value: T): Box<T> {
  return { value };
}

export function read<T>(value: Box<T>): T {
  return value.value;
}
