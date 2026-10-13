# 内置的第三方组件：LunaHook

本目录下的二进制文件来自 **LunaTranslator** 的官方发布包，未经修改，版本号见同目录 `VERSION`。

- 项目主页 / 源码：https://github.com/HIllya51/LunaTranslator （钩子源码在 `src/NativeImpl/LunaHook`）
- 许可证：**GNU General Public License v3.0**，全文见同目录 `COPYING`

## 只取了哪些文件

完整发布包约 65 MB，Fukidashi 只需要钩子核心：

- `LunaHost64.dll` —— 宿主，加载进 Fukidashi 进程，通过 `Luna_*` C 接口调用
- `LunaHook32.dll`、`LunaHook64.dll` —— 注入到游戏里的钩子，按游戏位数选用
- `LunaSubprocess32.exe`、`LunaSubprocess64.exe` —— 官方注入器（`dllinject <pid> <dll>`）

## 更新

```powershell
pnpm lunahook:update            # 最新版
pnpm lunahook:update v12.1.2.4  # 指定版本
```

脚本只覆盖上面这些文件以及 `COPYING`、`VERSION`，本说明不受影响。

## 分发义务

只要随 Fukidashi 一起分发这些二进制，就必须附上许可证全文（`COPYING`）、保留版权声明，
并提供对应源码或获取源码的途径（上面的仓库地址即可）。
