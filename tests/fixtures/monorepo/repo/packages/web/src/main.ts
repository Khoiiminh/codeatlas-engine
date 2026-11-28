import { greet } from "@mono/shared";
import { mount } from "@/mount";
import { pad } from "@/lib/format";

mount(pad(greet("web")));
