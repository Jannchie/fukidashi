<p align="center"><img src="src-tauri/icons/icon.svg" width="96" alt="" /></p>

<h1 align="center">Fukidashi 吹き出し</h1>

<p align="center">把视觉小说里的日文实时抓出来，边玩边读。</p>

- **抓文本**：连上正在运行的游戏，自动从多路钩子里挑出正文。钩子核心用的是 [LunaTranslator](https://github.com/HIllya51/LunaTranslator) 的 LunaHook。
- **振假名**：自动给汉字标读音，可以随时关掉。
- **翻译**：接任意 OpenAI 兼容接口（Ollama、LM Studio、llama.cpp、云端 API），译文流式显示。针对 Sakura / GalTransl 等专门的翻译模型，会自动套用它们各自的提示词格式。
- **记住游戏**：玩过的游戏一启动就自动连上，并沿用上次手动选定的那一路文本。
- **自动更新**：有新版本时，顶栏会出现更新按钮。

目前只支持 Windows。

## 安装

在 [Releases](https://github.com/Jannchie/fukidashi/releases) 下载 `Fukidashi_*_x64-setup.exe`。

如果游戏以管理员身份运行，Fukidashi 也需要以管理员身份运行。

## 本地翻译

推荐用 [Ollama](https://ollama.com) 跑 [Sakura-GalTransl](https://huggingface.co/SakuraLLM/Sakura-GalTransl-14B-v3.8)，14B 的 Q6_K 量化版约占 12 GB 显存。在翻译设置里选 Ollama 预设，再选模型即可。

## 开发

```sh
pnpm install
pnpm lunahook:fetch   # 拉取钩子核心（二进制不进仓库，版本记在 src-tauri/resources/lunahook/VERSION）
pnpm tauri dev
```

在普通浏览器里打开 http://localhost:1420 会使用模拟后端，方便调界面。

- `pnpm lunahook:update [tag]`：把钩子核心升级到 LunaTranslator 的最新版或指定版本
- `pnpm release <patch|minor|major|x.y.z> [--push]`：改版本号、提交、打 tag。推送 tag 后由 GitHub Actions 构建、签名、发布，已安装的客户端会据此收到更新

发布需要在仓库 Secrets 里设置 `TAURI_SIGNING_PRIVATE_KEY`（更新签名私钥）。

## 许可

GPL-3.0-or-later。附带的 LunaHook 二进制同样以 GPLv3 发布，见 `src-tauri/resources/lunahook/NOTICE.md`。
