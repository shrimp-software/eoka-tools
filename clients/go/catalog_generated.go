// Generated from the eoka operation catalog by scripts/generate-protocol.mjs. Do not edit.

package eoka

// Catalog operation method names for Page.Invoke.
const (
	OpBack                = "back"
	OpCaptchaDatadome     = "captcha.datadome"
	OpCaptchaInject       = "captcha.inject"
	OpCaptchaTurnstile    = "captcha.turnstile"
	OpClearCookies        = "clear_cookies"
	OpClick               = "click"
	OpClickShadow         = "click_shadow"
	OpCloneFrom           = "clone_from"
	OpConsole             = "console"
	OpContent             = "content"
	OpCookies             = "cookies"
	OpDeleteCookie        = "delete_cookie"
	OpDoubleClick         = "double_click"
	OpDownloadCancel      = "download.cancel"
	OpDownloadGet         = "download.get"
	OpDownloadList        = "download.list"
	OpDownloadStart       = "download.start"
	OpDownloadWait        = "download.wait"
	OpDumpStorage         = "dump_storage"
	OpEmulate             = "emulate"
	OpErrors              = "errors"
	OpEval                = "eval"
	OpExec                = "exec"
	OpExists              = "exists"
	OpFakeCamera          = "fake_camera"
	OpFetch               = "fetch"
	OpFill                = "fill"
	OpFind                = "find"
	OpForward             = "forward"
	OpFrameEval           = "frame_eval"
	OpFrames              = "frames"
	OpGetAttribute        = "get_attribute"
	OpGetText             = "get_text"
	OpHeaders             = "headers"
	OpHover               = "hover"
	OpInfo                = "info"
	OpInterceptAdd        = "intercept.add"
	OpInterceptList       = "intercept.list"
	OpInterceptLog        = "intercept.log"
	OpInterceptRemove     = "intercept.remove"
	OpJsAllow             = "js.allow"
	OpJsBlock             = "js.block"
	OpJsList              = "js.list"
	OpJsMode              = "js.mode"
	OpJsRemove            = "js.remove"
	OpKey                 = "key"
	OpKeyDown             = "key.down"
	OpKeyUp               = "key.up"
	OpLoadState           = "load_state"
	OpMouseDown           = "mouse.down"
	OpMouseMove           = "mouse.move"
	OpMouseUp             = "mouse.up"
	OpNetworkClear        = "network.clear"
	OpNetworkExport       = "network.export"
	OpNetworkLog          = "network.log"
	OpNetworkRecordStart  = "network.record.start"
	OpNetworkRecordStatus = "network.record.status"
	OpNetworkRecordStop   = "network.record.stop"
	OpNetworkShow         = "network.show"
	OpNetworkWait         = "network.wait"
	OpObserve             = "observe"
	OpOpen                = "open"
	OpReleaseAllInputs    = "release_all_inputs"
	OpReload              = "reload"
	OpSaveState           = "save_state"
	OpScreenshot          = "screenshot"
	OpScroll              = "scroll"
	OpSelect              = "select"
	OpSetCookie           = "set_cookie"
	OpSetStorage          = "set_storage"
	OpSnapshot            = "snapshot"
	OpSpaInfo             = "spa.info"
	OpSpaNavigate         = "spa.navigate"
	OpStorage             = "storage"
	OpText                = "text"
	OpWait                = "wait"
	OpWasmFind            = "wasm.find"
	OpWasmInfo            = "wasm.info"
	OpWasmRead            = "wasm.read"
	OpWasmWrite           = "wasm.write"
)

// CatalogMethods lists every catalog operation this client was generated with.
var CatalogMethods = []string{
	OpBack,
	OpCaptchaDatadome,
	OpCaptchaInject,
	OpCaptchaTurnstile,
	OpClearCookies,
	OpClick,
	OpClickShadow,
	OpCloneFrom,
	OpConsole,
	OpContent,
	OpCookies,
	OpDeleteCookie,
	OpDoubleClick,
	OpDownloadCancel,
	OpDownloadGet,
	OpDownloadList,
	OpDownloadStart,
	OpDownloadWait,
	OpDumpStorage,
	OpEmulate,
	OpErrors,
	OpEval,
	OpExec,
	OpExists,
	OpFakeCamera,
	OpFetch,
	OpFill,
	OpFind,
	OpForward,
	OpFrameEval,
	OpFrames,
	OpGetAttribute,
	OpGetText,
	OpHeaders,
	OpHover,
	OpInfo,
	OpInterceptAdd,
	OpInterceptList,
	OpInterceptLog,
	OpInterceptRemove,
	OpJsAllow,
	OpJsBlock,
	OpJsList,
	OpJsMode,
	OpJsRemove,
	OpKey,
	OpKeyDown,
	OpKeyUp,
	OpLoadState,
	OpMouseDown,
	OpMouseMove,
	OpMouseUp,
	OpNetworkClear,
	OpNetworkExport,
	OpNetworkLog,
	OpNetworkRecordStart,
	OpNetworkRecordStatus,
	OpNetworkRecordStop,
	OpNetworkShow,
	OpNetworkWait,
	OpObserve,
	OpOpen,
	OpReleaseAllInputs,
	OpReload,
	OpSaveState,
	OpScreenshot,
	OpScroll,
	OpSelect,
	OpSetCookie,
	OpSetStorage,
	OpSnapshot,
	OpSpaInfo,
	OpSpaNavigate,
	OpStorage,
	OpText,
	OpWait,
	OpWasmFind,
	OpWasmInfo,
	OpWasmRead,
	OpWasmWrite,
}
