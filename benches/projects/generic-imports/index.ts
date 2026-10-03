import { box, read } from "./schema";
import type { Box, Item, Row, Unbox } from "./schema";

export const item: Row<Item> = {
  id: box("one"), count: box(1), active: box(true),
};
export const restored: Unbox<Row<Item>> = {
  id: read(item.id), count: read(item.count), active: read(item.active),
};
export const nested: Box<Box<Row<Item>>> = box(box(item));

// A benchmark that stops checking must fail its diagnostic fingerprint.
export const bad: number = "diagnostic control";
