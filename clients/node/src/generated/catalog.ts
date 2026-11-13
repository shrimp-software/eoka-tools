// Generated from the eoka operation catalog by scripts/generate-protocol.mjs. Do not edit.
import type { CallOptions } from "../client.js";
import type { MethodParams, MethodResults } from "./protocol.js";

export const catalogMethods = ["back","captcha.datadome","captcha.inject","captcha.turnstile","clear_cookies","click","click_shadow","clone_from","console","content","cookies","delete_cookie","double_click","download.cancel","download.get","download.list","download.start","download.wait","dump_storage","emulate","errors","eval","exec","exists","fake_camera","fetch","fill","find","forward","frame_eval","frames","get_attribute","get_text","headers","hover","info","intercept.add","intercept.list","intercept.log","intercept.remove","js.allow","js.block","js.list","js.mode","js.remove","key","key.down","key.up","load_state","mouse.down","mouse.move","mouse.up","network.clear","network.export","network.log","network.record.start","network.record.status","network.record.stop","network.show","network.wait","observe","open","release_all_inputs","reload","save_state","screenshot","scroll","select","set_cookie","set_storage","snapshot","spa.info","spa.navigate","storage","text","wait","wasm.find","wasm.info","wasm.read","wasm.write"] as const;
export type CatalogMethod = (typeof catalogMethods)[number];
/** An operation's input without the page ID, which the page supplies. */
export type CatalogInput<M extends CatalogMethod> = Omit<MethodParams[M], "pageId">;

export interface CatalogPage {
  readonly id: string;
  invoke<M extends CatalogMethod>(method: M, params: MethodParams[M], options?: CallOptions): Promise<MethodResults[M]>;
}

/** Every catalog operation as a method bound to one page. */
export function catalogOperations(page: CatalogPage) {
  const call = <M extends CatalogMethod>(method: M, input: CatalogInput<M>, options?: CallOptions) =>
    page.invoke(method, { ...input, pageId: page.id } as MethodParams[M], options);
  return {
    back: (input: CatalogInput<"back"> = {}, options?: CallOptions) => call("back", input, options),
    captchaDatadome: (input: CatalogInput<"captcha.datadome"> = {}, options?: CallOptions) => call("captcha.datadome", input, options),
    captchaInject: (input: CatalogInput<"captcha.inject">, options?: CallOptions) => call("captcha.inject", input, options),
    captchaTurnstile: (input: CatalogInput<"captcha.turnstile"> = {}, options?: CallOptions) => call("captcha.turnstile", input, options),
    clearCookies: (input: CatalogInput<"clear_cookies"> = {}, options?: CallOptions) => call("clear_cookies", input, options),
    click: (input: CatalogInput<"click">, options?: CallOptions) => call("click", input, options),
    clickShadow: (input: CatalogInput<"click_shadow">, options?: CallOptions) => call("click_shadow", input, options),
    cloneFrom: (input: CatalogInput<"clone_from">, options?: CallOptions) => call("clone_from", input, options),
    console: (input: CatalogInput<"console"> = {}, options?: CallOptions) => call("console", input, options),
    content: (input: CatalogInput<"content"> = {}, options?: CallOptions) => call("content", input, options),
    cookies: (input: CatalogInput<"cookies"> = {}, options?: CallOptions) => call("cookies", input, options),
    deleteCookie: (input: CatalogInput<"delete_cookie">, options?: CallOptions) => call("delete_cookie", input, options),
    doubleClick: (input: CatalogInput<"double_click">, options?: CallOptions) => call("double_click", input, options),
    downloadCancel: (input: CatalogInput<"download.cancel">, options?: CallOptions) => call("download.cancel", input, options),
    downloadGet: (input: CatalogInput<"download.get">, options?: CallOptions) => call("download.get", input, options),
    downloadList: (input: CatalogInput<"download.list"> = {}, options?: CallOptions) => call("download.list", input, options),
    downloadStart: (input: CatalogInput<"download.start">, options?: CallOptions) => call("download.start", input, options),
    downloadWait: (input: CatalogInput<"download.wait">, options?: CallOptions) => call("download.wait", input, options),
    dumpStorage: (input: CatalogInput<"dump_storage"> = {}, options?: CallOptions) => call("dump_storage", input, options),
    emulate: (input: CatalogInput<"emulate"> = {}, options?: CallOptions) => call("emulate", input, options),
    errors: (input: CatalogInput<"errors"> = {}, options?: CallOptions) => call("errors", input, options),
    eval: (input: CatalogInput<"eval"> = {}, options?: CallOptions) => call("eval", input, options),
    exec: (input: CatalogInput<"exec"> = {}, options?: CallOptions) => call("exec", input, options),
    exists: (input: CatalogInput<"exists">, options?: CallOptions) => call("exists", input, options),
    fakeCamera: (input: CatalogInput<"fake_camera">, options?: CallOptions) => call("fake_camera", input, options),
    fetch: (input: CatalogInput<"fetch">, options?: CallOptions) => call("fetch", input, options),
    fill: (input: CatalogInput<"fill">, options?: CallOptions) => call("fill", input, options),
    find: (input: CatalogInput<"find">, options?: CallOptions) => call("find", input, options),
    forward: (input: CatalogInput<"forward"> = {}, options?: CallOptions) => call("forward", input, options),
    frameEval: (input: CatalogInput<"frame_eval">, options?: CallOptions) => call("frame_eval", input, options),
    frames: (input: CatalogInput<"frames"> = {}, options?: CallOptions) => call("frames", input, options),
    getAttribute: (input: CatalogInput<"get_attribute">, options?: CallOptions) => call("get_attribute", input, options),
    getText: (input: CatalogInput<"get_text">, options?: CallOptions) => call("get_text", input, options),
    headers: (input: CatalogInput<"headers">, options?: CallOptions) => call("headers", input, options),
    hover: (input: CatalogInput<"hover">, options?: CallOptions) => call("hover", input, options),
    info: (input: CatalogInput<"info"> = {}, options?: CallOptions) => call("info", input, options),
    interceptAdd: (input: CatalogInput<"intercept.add">, options?: CallOptions) => call("intercept.add", input, options),
    interceptList: (input: CatalogInput<"intercept.list"> = {}, options?: CallOptions) => call("intercept.list", input, options),
    interceptLog: (input: CatalogInput<"intercept.log"> = {}, options?: CallOptions) => call("intercept.log", input, options),
    interceptRemove: (input: CatalogInput<"intercept.remove">, options?: CallOptions) => call("intercept.remove", input, options),
    jsAllow: (input: CatalogInput<"js.allow">, options?: CallOptions) => call("js.allow", input, options),
    jsBlock: (input: CatalogInput<"js.block">, options?: CallOptions) => call("js.block", input, options),
    jsList: (input: CatalogInput<"js.list"> = {}, options?: CallOptions) => call("js.list", input, options),
    jsMode: (input: CatalogInput<"js.mode">, options?: CallOptions) => call("js.mode", input, options),
    jsRemove: (input: CatalogInput<"js.remove">, options?: CallOptions) => call("js.remove", input, options),
    key: (input: CatalogInput<"key">, options?: CallOptions) => call("key", input, options),
    keyDown: (input: CatalogInput<"key.down">, options?: CallOptions) => call("key.down", input, options),
    keyUp: (input: CatalogInput<"key.up">, options?: CallOptions) => call("key.up", input, options),
    loadState: (input: CatalogInput<"load_state">, options?: CallOptions) => call("load_state", input, options),
    mouseDown: (input: CatalogInput<"mouse.down">, options?: CallOptions) => call("mouse.down", input, options),
    mouseMove: (input: CatalogInput<"mouse.move">, options?: CallOptions) => call("mouse.move", input, options),
    mouseUp: (input: CatalogInput<"mouse.up">, options?: CallOptions) => call("mouse.up", input, options),
    networkClear: (input: CatalogInput<"network.clear"> = {}, options?: CallOptions) => call("network.clear", input, options),
    networkExport: (input: CatalogInput<"network.export">, options?: CallOptions) => call("network.export", input, options),
    networkLog: (input: CatalogInput<"network.log"> = {}, options?: CallOptions) => call("network.log", input, options),
    networkRecordStart: (input: CatalogInput<"network.record.start"> = {}, options?: CallOptions) => call("network.record.start", input, options),
    networkRecordStatus: (input: CatalogInput<"network.record.status"> = {}, options?: CallOptions) => call("network.record.status", input, options),
    networkRecordStop: (input: CatalogInput<"network.record.stop"> = {}, options?: CallOptions) => call("network.record.stop", input, options),
    networkShow: (input: CatalogInput<"network.show">, options?: CallOptions) => call("network.show", input, options),
    networkWait: (input: CatalogInput<"network.wait"> = {}, options?: CallOptions) => call("network.wait", input, options),
    observe: (input: CatalogInput<"observe"> = {}, options?: CallOptions) => call("observe", input, options),
    open: (input: CatalogInput<"open">, options?: CallOptions) => call("open", input, options),
    releaseAllInputs: (input: CatalogInput<"release_all_inputs"> = {}, options?: CallOptions) => call("release_all_inputs", input, options),
    reload: (input: CatalogInput<"reload"> = {}, options?: CallOptions) => call("reload", input, options),
    saveState: (input: CatalogInput<"save_state">, options?: CallOptions) => call("save_state", input, options),
    screenshot: (input: CatalogInput<"screenshot"> = {}, options?: CallOptions) => call("screenshot", input, options),
    scroll: (input: CatalogInput<"scroll">, options?: CallOptions) => call("scroll", input, options),
    select: (input: CatalogInput<"select">, options?: CallOptions) => call("select", input, options),
    setCookie: (input: CatalogInput<"set_cookie">, options?: CallOptions) => call("set_cookie", input, options),
    setStorage: (input: CatalogInput<"set_storage">, options?: CallOptions) => call("set_storage", input, options),
    snapshot: (input: CatalogInput<"snapshot"> = {}, options?: CallOptions) => call("snapshot", input, options),
    spaInfo: (input: CatalogInput<"spa.info"> = {}, options?: CallOptions) => call("spa.info", input, options),
    spaNavigate: (input: CatalogInput<"spa.navigate">, options?: CallOptions) => call("spa.navigate", input, options),
    storage: (input: CatalogInput<"storage"> = {}, options?: CallOptions) => call("storage", input, options),
    text: (input: CatalogInput<"text"> = {}, options?: CallOptions) => call("text", input, options),
    wait: (input: CatalogInput<"wait"> = {}, options?: CallOptions) => call("wait", input, options),
    wasmFind: (input: CatalogInput<"wasm.find">, options?: CallOptions) => call("wasm.find", input, options),
    wasmInfo: (input: CatalogInput<"wasm.info"> = {}, options?: CallOptions) => call("wasm.info", input, options),
    wasmRead: (input: CatalogInput<"wasm.read">, options?: CallOptions) => call("wasm.read", input, options),
    wasmWrite: (input: CatalogInput<"wasm.write">, options?: CallOptions) => call("wasm.write", input, options),
  };
}

export type CatalogOperations = ReturnType<typeof catalogOperations>;
