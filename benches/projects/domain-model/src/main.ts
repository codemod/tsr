import { Model000Service, makeModel000, describeModel000Event } from "./model000";
import { Model001Service, makeModel001, describeModel001Event } from "./model001";
import { Model002Service, makeModel002, describeModel002Event } from "./model002";
import { Model003Service, makeModel003, describeModel003Event } from "./model003";
import { Model004Service, makeModel004, describeModel004Event } from "./model004";
import { Model005Service, makeModel005, describeModel005Event } from "./model005";
import { Model006Service, makeModel006, describeModel006Event } from "./model006";
import { Model007Service, makeModel007, describeModel007Event } from "./model007";
import { Model008Service, makeModel008, describeModel008Event } from "./model008";
import { Model009Service, makeModel009, describeModel009Event } from "./model009";
import { Model010Service, makeModel010, describeModel010Event } from "./model010";
import { Model011Service, makeModel011, describeModel011Event } from "./model011";
import { Model012Service, makeModel012, describeModel012Event } from "./model012";
import { Model013Service, makeModel013, describeModel013Event } from "./model013";
import { Model014Service, makeModel014, describeModel014Event } from "./model014";
import { Model015Service, makeModel015, describeModel015Event } from "./model015";
import { Model016Service, makeModel016, describeModel016Event } from "./model016";
import { Model017Service, makeModel017, describeModel017Event } from "./model017";
import { Model018Service, makeModel018, describeModel018Event } from "./model018";
import { Model019Service, makeModel019, describeModel019Event } from "./model019";
import { Model020Service, makeModel020, describeModel020Event } from "./model020";
import { Model021Service, makeModel021, describeModel021Event } from "./model021";
import { Model022Service, makeModel022, describeModel022Event } from "./model022";
import { Model023Service, makeModel023, describeModel023Event } from "./model023";
import { Model024Service, makeModel024, describeModel024Event } from "./model024";
import { Model025Service, makeModel025, describeModel025Event } from "./model025";
import { Model026Service, makeModel026, describeModel026Event } from "./model026";
import { Model027Service, makeModel027, describeModel027Event } from "./model027";
import { Model028Service, makeModel028, describeModel028Event } from "./model028";
import { Model029Service, makeModel029, describeModel029Event } from "./model029";
import { Model030Service, makeModel030, describeModel030Event } from "./model030";
import { Model031Service, makeModel031, describeModel031Event } from "./model031";
import { Model032Service, makeModel032, describeModel032Event } from "./model032";
import { Model033Service, makeModel033, describeModel033Event } from "./model033";
import { Model034Service, makeModel034, describeModel034Event } from "./model034";
import { Model035Service, makeModel035, describeModel035Event } from "./model035";
import { Model036Service, makeModel036, describeModel036Event } from "./model036";
import { Model037Service, makeModel037, describeModel037Event } from "./model037";
import { Model038Service, makeModel038, describeModel038Event } from "./model038";
import { Model039Service, makeModel039, describeModel039Event } from "./model039";

export function run(): string[] {
  const out: string[] = [];
  const service000 = new Model000Service();
  const created000 = service000.create(makeModel000("a0", "First 0"));
  if (created000.ok) {
    out.push(describeModel000Event({ kind: "created", item: created000.value }));
    service000.move(created000.value.id, "archived");
  } else {
    out.push(...created000.error);
  }
  const service001 = new Model001Service();
  const created001 = service001.create(makeModel001("a1", "First 1"));
  if (created001.ok) {
    out.push(describeModel001Event({ kind: "created", item: created001.value }));
    service001.move(created001.value.id, "archived");
  } else {
    out.push(...created001.error);
  }
  const service002 = new Model002Service();
  const created002 = service002.create(makeModel002("a2", "First 2"));
  if (created002.ok) {
    out.push(describeModel002Event({ kind: "created", item: created002.value }));
    service002.move(created002.value.id, "archived");
  } else {
    out.push(...created002.error);
  }
  const service003 = new Model003Service();
  const created003 = service003.create(makeModel003("a3", "First 3"));
  if (created003.ok) {
    out.push(describeModel003Event({ kind: "created", item: created003.value }));
    service003.move(created003.value.id, "archived");
  } else {
    out.push(...created003.error);
  }
  const service004 = new Model004Service();
  const created004 = service004.create(makeModel004("a4", "First 4"));
  if (created004.ok) {
    out.push(describeModel004Event({ kind: "created", item: created004.value }));
    service004.move(created004.value.id, "archived");
  } else {
    out.push(...created004.error);
  }
  const service005 = new Model005Service();
  const created005 = service005.create(makeModel005("a5", "First 5"));
  if (created005.ok) {
    out.push(describeModel005Event({ kind: "created", item: created005.value }));
    service005.move(created005.value.id, "archived");
  } else {
    out.push(...created005.error);
  }
  const service006 = new Model006Service();
  const created006 = service006.create(makeModel006("a6", "First 6"));
  if (created006.ok) {
    out.push(describeModel006Event({ kind: "created", item: created006.value }));
    service006.move(created006.value.id, "archived");
  } else {
    out.push(...created006.error);
  }
  const service007 = new Model007Service();
  const created007 = service007.create(makeModel007("a7", "First 7"));
  if (created007.ok) {
    out.push(describeModel007Event({ kind: "created", item: created007.value }));
    service007.move(created007.value.id, "archived");
  } else {
    out.push(...created007.error);
  }
  const service008 = new Model008Service();
  const created008 = service008.create(makeModel008("a8", "First 8"));
  if (created008.ok) {
    out.push(describeModel008Event({ kind: "created", item: created008.value }));
    service008.move(created008.value.id, "archived");
  } else {
    out.push(...created008.error);
  }
  const service009 = new Model009Service();
  const created009 = service009.create(makeModel009("a9", "First 9"));
  if (created009.ok) {
    out.push(describeModel009Event({ kind: "created", item: created009.value }));
    service009.move(created009.value.id, "archived");
  } else {
    out.push(...created009.error);
  }
  const service010 = new Model010Service();
  const created010 = service010.create(makeModel010("a10", "First 10"));
  if (created010.ok) {
    out.push(describeModel010Event({ kind: "created", item: created010.value }));
    service010.move(created010.value.id, "archived");
  } else {
    out.push(...created010.error);
  }
  const service011 = new Model011Service();
  const created011 = service011.create(makeModel011("a11", "First 11"));
  if (created011.ok) {
    out.push(describeModel011Event({ kind: "created", item: created011.value }));
    service011.move(created011.value.id, "archived");
  } else {
    out.push(...created011.error);
  }
  const service012 = new Model012Service();
  const created012 = service012.create(makeModel012("a12", "First 12"));
  if (created012.ok) {
    out.push(describeModel012Event({ kind: "created", item: created012.value }));
    service012.move(created012.value.id, "archived");
  } else {
    out.push(...created012.error);
  }
  const service013 = new Model013Service();
  const created013 = service013.create(makeModel013("a13", "First 13"));
  if (created013.ok) {
    out.push(describeModel013Event({ kind: "created", item: created013.value }));
    service013.move(created013.value.id, "archived");
  } else {
    out.push(...created013.error);
  }
  const service014 = new Model014Service();
  const created014 = service014.create(makeModel014("a14", "First 14"));
  if (created014.ok) {
    out.push(describeModel014Event({ kind: "created", item: created014.value }));
    service014.move(created014.value.id, "archived");
  } else {
    out.push(...created014.error);
  }
  const service015 = new Model015Service();
  const created015 = service015.create(makeModel015("a15", "First 15"));
  if (created015.ok) {
    out.push(describeModel015Event({ kind: "created", item: created015.value }));
    service015.move(created015.value.id, "archived");
  } else {
    out.push(...created015.error);
  }
  const service016 = new Model016Service();
  const created016 = service016.create(makeModel016("a16", "First 16"));
  if (created016.ok) {
    out.push(describeModel016Event({ kind: "created", item: created016.value }));
    service016.move(created016.value.id, "archived");
  } else {
    out.push(...created016.error);
  }
  const service017 = new Model017Service();
  const created017 = service017.create(makeModel017("a17", "First 17"));
  if (created017.ok) {
    out.push(describeModel017Event({ kind: "created", item: created017.value }));
    service017.move(created017.value.id, "archived");
  } else {
    out.push(...created017.error);
  }
  const service018 = new Model018Service();
  const created018 = service018.create(makeModel018("a18", "First 18"));
  if (created018.ok) {
    out.push(describeModel018Event({ kind: "created", item: created018.value }));
    service018.move(created018.value.id, "archived");
  } else {
    out.push(...created018.error);
  }
  const service019 = new Model019Service();
  const created019 = service019.create(makeModel019("a19", "First 19"));
  if (created019.ok) {
    out.push(describeModel019Event({ kind: "created", item: created019.value }));
    service019.move(created019.value.id, "archived");
  } else {
    out.push(...created019.error);
  }
  const service020 = new Model020Service();
  const created020 = service020.create(makeModel020("a20", "First 20"));
  if (created020.ok) {
    out.push(describeModel020Event({ kind: "created", item: created020.value }));
    service020.move(created020.value.id, "archived");
  } else {
    out.push(...created020.error);
  }
  const service021 = new Model021Service();
  const created021 = service021.create(makeModel021("a21", "First 21"));
  if (created021.ok) {
    out.push(describeModel021Event({ kind: "created", item: created021.value }));
    service021.move(created021.value.id, "archived");
  } else {
    out.push(...created021.error);
  }
  const service022 = new Model022Service();
  const created022 = service022.create(makeModel022("a22", "First 22"));
  if (created022.ok) {
    out.push(describeModel022Event({ kind: "created", item: created022.value }));
    service022.move(created022.value.id, "archived");
  } else {
    out.push(...created022.error);
  }
  const service023 = new Model023Service();
  const created023 = service023.create(makeModel023("a23", "First 23"));
  if (created023.ok) {
    out.push(describeModel023Event({ kind: "created", item: created023.value }));
    service023.move(created023.value.id, "archived");
  } else {
    out.push(...created023.error);
  }
  const service024 = new Model024Service();
  const created024 = service024.create(makeModel024("a24", "First 24"));
  if (created024.ok) {
    out.push(describeModel024Event({ kind: "created", item: created024.value }));
    service024.move(created024.value.id, "archived");
  } else {
    out.push(...created024.error);
  }
  const service025 = new Model025Service();
  const created025 = service025.create(makeModel025("a25", "First 25"));
  if (created025.ok) {
    out.push(describeModel025Event({ kind: "created", item: created025.value }));
    service025.move(created025.value.id, "archived");
  } else {
    out.push(...created025.error);
  }
  const service026 = new Model026Service();
  const created026 = service026.create(makeModel026("a26", "First 26"));
  if (created026.ok) {
    out.push(describeModel026Event({ kind: "created", item: created026.value }));
    service026.move(created026.value.id, "archived");
  } else {
    out.push(...created026.error);
  }
  const service027 = new Model027Service();
  const created027 = service027.create(makeModel027("a27", "First 27"));
  if (created027.ok) {
    out.push(describeModel027Event({ kind: "created", item: created027.value }));
    service027.move(created027.value.id, "archived");
  } else {
    out.push(...created027.error);
  }
  const service028 = new Model028Service();
  const created028 = service028.create(makeModel028("a28", "First 28"));
  if (created028.ok) {
    out.push(describeModel028Event({ kind: "created", item: created028.value }));
    service028.move(created028.value.id, "archived");
  } else {
    out.push(...created028.error);
  }
  const service029 = new Model029Service();
  const created029 = service029.create(makeModel029("a29", "First 29"));
  if (created029.ok) {
    out.push(describeModel029Event({ kind: "created", item: created029.value }));
    service029.move(created029.value.id, "archived");
  } else {
    out.push(...created029.error);
  }
  const service030 = new Model030Service();
  const created030 = service030.create(makeModel030("a30", "First 30"));
  if (created030.ok) {
    out.push(describeModel030Event({ kind: "created", item: created030.value }));
    service030.move(created030.value.id, "archived");
  } else {
    out.push(...created030.error);
  }
  const service031 = new Model031Service();
  const created031 = service031.create(makeModel031("a31", "First 31"));
  if (created031.ok) {
    out.push(describeModel031Event({ kind: "created", item: created031.value }));
    service031.move(created031.value.id, "archived");
  } else {
    out.push(...created031.error);
  }
  const service032 = new Model032Service();
  const created032 = service032.create(makeModel032("a32", "First 32"));
  if (created032.ok) {
    out.push(describeModel032Event({ kind: "created", item: created032.value }));
    service032.move(created032.value.id, "archived");
  } else {
    out.push(...created032.error);
  }
  const service033 = new Model033Service();
  const created033 = service033.create(makeModel033("a33", "First 33"));
  if (created033.ok) {
    out.push(describeModel033Event({ kind: "created", item: created033.value }));
    service033.move(created033.value.id, "archived");
  } else {
    out.push(...created033.error);
  }
  const service034 = new Model034Service();
  const created034 = service034.create(makeModel034("a34", "First 34"));
  if (created034.ok) {
    out.push(describeModel034Event({ kind: "created", item: created034.value }));
    service034.move(created034.value.id, "archived");
  } else {
    out.push(...created034.error);
  }
  const service035 = new Model035Service();
  const created035 = service035.create(makeModel035("a35", "First 35"));
  if (created035.ok) {
    out.push(describeModel035Event({ kind: "created", item: created035.value }));
    service035.move(created035.value.id, "archived");
  } else {
    out.push(...created035.error);
  }
  const service036 = new Model036Service();
  const created036 = service036.create(makeModel036("a36", "First 36"));
  if (created036.ok) {
    out.push(describeModel036Event({ kind: "created", item: created036.value }));
    service036.move(created036.value.id, "archived");
  } else {
    out.push(...created036.error);
  }
  const service037 = new Model037Service();
  const created037 = service037.create(makeModel037("a37", "First 37"));
  if (created037.ok) {
    out.push(describeModel037Event({ kind: "created", item: created037.value }));
    service037.move(created037.value.id, "archived");
  } else {
    out.push(...created037.error);
  }
  const service038 = new Model038Service();
  const created038 = service038.create(makeModel038("a38", "First 38"));
  if (created038.ok) {
    out.push(describeModel038Event({ kind: "created", item: created038.value }));
    service038.move(created038.value.id, "archived");
  } else {
    out.push(...created038.error);
  }
  const service039 = new Model039Service();
  const created039 = service039.create(makeModel039("a39", "First 39"));
  if (created039.ok) {
    out.push(describeModel039Event({ kind: "created", item: created039.value }));
    service039.move(created039.value.id, "archived");
  } else {
    out.push(...created039.error);
  }
  return out;
}

// A benchmark that stops checking must fail its diagnostic fingerprint.
export const control: number = run().join("\n");
