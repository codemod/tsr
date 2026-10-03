import { box, read } from "./schema";
import type { Box, Row } from "./schema";
import { item, nested } from "./index";

export const another: Row<{ name: string; rating: number }> = {
  name: box(read(item.id)), rating: box(read(item.count)),
};
export const recovered = read(read(nested));
export const recursive: Box<number> = { value: 1, previous: { value: 2 } };
