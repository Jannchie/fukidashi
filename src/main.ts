import "./style.css";
import { createApp } from "vue";
import { JIconPlugin } from "@jannchie/icons-vue";
import App from "./App.vue";
import { reportError } from "./composables/useFukidashi";

const app = createApp(App);
// 界面以小尺寸图标为主，开 hinting 让 1x 屏上的线条对齐像素、不发虚
app.use(JIconPlugin, { size: 16, hinting: true });

// 组件渲染期的异常默认只进控制台，桌面应用里等于石沉大海
app.config.errorHandler = (err) => reportError(err);
window.addEventListener("error", (e) => reportError(e.error ?? e.message));
window.addEventListener("unhandledrejection", (e) => reportError(e.reason));

// 在普通浏览器里打开 dev 页面时模拟后端，便于调界面；正式包和 Tauri 窗口里都不会走这里
if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
  import("./dev/browserMock").then(() => app.mount("#app"));
} else {
  app.mount("#app");
}
