# Echo 应用内更新

支持 macOS arm64、macOS x86_64 和 Windows x64 安装版。两个平台都使用 Minisign 验证更新包。macOS 应用使用 ad-hoc 代码签名。Rune 负责 UI。
`src/update/transport.rs` 负责协议与签名，`macos.rs` 和 `windows.rs` 分别负责各自平台的安装，
`mod.rs` 负责独立工作线程和状态机。更新模块不直接读写笔记。

## 使用行为

自动检查默认开启：启动 10 秒后检查，此后每 12 小时检查。发现新版后，侧栏自动出现下载入口，无需先进入设置手动检测。也可以在「设置 → 软件更新」立即检查，或关闭自动检查；已经明确关闭的偏好会继续保留。
自动检查仅请求公开版本清单，不上传笔记；下载和重启安装仍由用户点击确认。
macOS 和 Windows 在发现新版后才会在侧栏设置图标上方显示更新按钮。点击按钮下载，原位用水波和整数百分比显示进度，达到 100% 后进入校验。准备就绪后点击「重启更新」才会安装。下载或安装失败时，点击图标可查看原因并重新检查。
自动检查失败显示在更新页；不会切换更新源或绕过签名。

下载到应用所在目录的独立私有临时目录，验证大小、Minisign 签名、签名中的
版本及平台名称，然后解压。新应用必须通过 codesign 完整性检查，且 Bundle ID、版本及架构均匹配。
发布者身份由应用内置 Minisign 公钥认证，不依赖 Apple Team ID。安装辅助进程再次验证，报告就绪后，
主程序完成编辑并等待保存队列落盘。保存失败会取消安装，保持应用打开。
辅助进程收到安装确认并等待主进程退出后，以 `renameatx_np(RENAME_SWAP)`
原子交换应用目录，再重启。交换失败不改变旧应用，没有复制覆盖的备用路径。
成功启动请求后清理暂存目录；安装后的错误记录在 `.echo-update-*/install.log`。
`open` 成功仅表示 Launch Services 接受启动请求，不等于新版健康检查通过。

## 自动递增发版版本

在 GitHub 仓库的 **Actions → Prepare release → Run workflow** 中选择 `main`，然后选择递增方式：

- `patch`（默认）：`0.0.1 → 0.0.2`。
- `minor`：`0.0.1 → 0.1.0`，补丁号归零。
- `major`：`0.0.1 → 1.0.0`，次版本号和补丁号归零。

工作流会检查版本一致性，自动计算新版本，同步 `Cargo.toml`、`Cargo.lock`、Mac 的 `Info.plist`、Windows manifest 和 README 下载链接，再以 `chore(release): bump version to X.Y.Z` 提交到 `main`，并创建 `vX.Y.Z` tag。提交和 tag 一起推送；推送遇到冲突时会失败，不覆盖远端改动。运行工作流的 token 需要仓库内容写入权限，仓库分支规则也须允许该自动提交。

每次发版只运行一次 **Prepare release**。准备完成后，本地执行 `git pull --ff-only` 获取新版本；Mac 与 Windows 都从生成的同一个 tag 构建。现有 `release.sh`、`release-windows.ps1` 和普通构建脚本读取已准备好的版本，不重复递增，因此重试构建或更换平台不会额外增加版本号。

此工作流准备版本和 tag，不上传安装包或发布 GitHub Release。由 `GITHUB_TOKEN` 推送的提交与 tag 不会自动触发普通 `push` CI；准备好版本后，可分别手动运行 **macOS** 和 **Windows** 工作流并选择相应 tag，获取同一版本的构建产物。

本地也可以用 Python 3.11 或更新版本自动同步版本（本地脚本只更新文件，不自动提交或创建 tag）：

```sh
python3 scripts/bump_version.py                 # 自动递增 patch
python3 scripts/bump_version.py --part minor    # 自动递增 minor
python3 scripts/bump_version.py --part major    # 自动递增 major
python3 scripts/bump_version.py --dry-run       # 预览下一个版本，不修改文件
python3 scripts/bump_version.py --check         # 只检查当前各处版本是否一致
```

## 发行前提

1. 准备一个公开的 GitHub Releases 仓库，源代码仓库可保持私有。
2. 安装 Minisign，执行 `minisign -G -p /安全路径/echo-update.pub -s /安全路径/echo-update.key`。
   使用加密私钥并离线备份。私钥、密码不得加入仓库。
3. 首个包含更新公钥和发布源的版本需手动安装一次。之后更新必须由同一密钥签署。

这不提供 Apple 公证，也不关闭或修改 Gatekeeper。首次下载安装时 macOS 可能
要求在「系统设置 → 隐私与安全性」手动允许打开。企业策略仍可能禁止运行。
更新器不会移除 quarantine 属性或执行系统安全设置修改命令。

正式构建示例（路径与身份由发布者填写）：

```sh
ECHO_UPDATE_KEY_FILE='/安全路径/echo-update.pub'
MINISIGN_SECRET_KEY='/安全路径/echo-update.key'
ECHO_UPDATE_REPOSITORY='https://github.com/账号/公开发布仓库'
export ECHO_UPDATE_KEY_FILE MINISIGN_SECRET_KEY ECHO_UPDATE_REPOSITORY
bash scripts/release.sh
```

脚本读取 Cargo.toml 版本，构建、ad-hoc 代码签名、打包并通过 Minisign 签署更新包。
公钥编译进二进制；签名失败、缺少密钥或公开发布源时立即终止。
不提供跳过校验模式，不自动创建、上传或发布 GitHub Release。
`scripts/bundle.sh` 仍用于本地 ad-hoc 开发包，不是发布脚本。

`bash scripts/build-macos.sh` 生成 `dist/Echo-VERSION-macos-arm64.zip`（在 Intel Mac 上为 `macos-x86_64`）及 SHA-256 校验文件，并验证应用架构和 ad-hoc 代码签名。它不生成 Minisign 更新签名；正式发行时仍须使用 `release.sh` 或在发行机器上为相同 CI 产物签名并生成更新清单。**macOS** 工作流使用 Apple Silicon 构建机执行测试和此打包脚本，上传 `Echo-macos-arm64` artifact。

## 发行版的源码交付

Echo 的原创代码采用 AGPL-3.0-only。发布二进制下载时，在同一发布页面同时提供与该二进制版本对应的完整源码下载，并清楚标明其对应关系。源码应包含本项目代码、随附的 `vendor/` 依赖、本地补丁、构建与打包脚本及所需资源；保留 `LICENSE`、`THIRD_PARTY.md` 和第三方许可声明。若使用 GitHub 自动生成的源码归档，确认发布 tag 指向实际构建所用的源码版本。

macOS 打包脚本将协议放入 `Echo.app/Contents/Resources/LICENSE`；Windows 便携包与安装目录均附带 `LICENSE`。附带许可证全文不代替提供对应源码的义务。

## 发布协议

源通过构建变量 `ECHO_UPDATE_REPOSITORY` 指定公开 GitHub Releases 仓库，并编译进应用。
未配置更新源或公钥的构建不会发起更新请求；不使用运行时环境变量切换信任源。
源代码仓库可以保持私有，安装包使用独立公开仓库。发布脚本会验证该仓库公开可访问。
保持仓库地址、Bundle ID 和 Minisign 公钥稳定。
协议当前仅接受三段稳定版本号，如 `0.11.0`，不接受预发布标签。

每个版本先创建草稿 Release，tag 必须为 `v<version>`。上传脚本产生的：

- `Echo-<version>-macos-arm64.zip` / `Echo-<version>-macos-x86_64.zip`
- 每个包对应的 `.minisig`、`.sha256`
- `update-macos-arm64.json` / `update-macos-x86_64.json`

只发布已构建并验证的平台。需要同时服务两种架构时，两个平台全部上传完成，
再一次性发布草稿并设为 latest。客户端读取
`/releases/latest/download/update-<platform>.json`；包 URL 从固定仓库、版本、
平台推导，不接受清单提供的任意 URL。GitHub CDN 重定向仅允许 HTTPS。
仓库私有或尚未发布对应清单时，检查会明确失败；客户端不内置 GitHub token。

清单字段：`version`、`platform`、`size`、`signature`（完整 Minisign 文本）。
签名的 trusted comment 必须严格等于包名，防止旧包冒充新版。
清单自身未签名；签名绑定包、版本与平台，清单大小值还需与下载文件一致。
不允许自动降级。更换发布密钥需要设计单独的密钥迁移流程。

## 验证边界

自动测试覆盖签名篡改、错误公钥、截断、平台/版本不匹配、非法清单字段、
状态机完成/断线、保存屏障以及 macOS 原子交换成功与失败。
发行前仍须用两份真实 Minisign 签名的发行包，分别验证 arm64 / x86_64
的完整下载与重启、无写入权限目录、无网络和新版启动行为。


## Windows x64

`pwsh -File scripts/build-windows.ps1 -Installer` 生成便携 ZIP 和 Inno Setup 安装包。
安装包是当前用户安装，不请求管理员权限；卸载保留 `%LOCALAPPDATA%\Echo`。
便携版供测试使用，不执行应用内更新。

正式发布在 Windows 上运行（需要 Minisign 与 Inno Setup 6）：

```powershell
pwsh -File scripts/release-windows.ps1 -Repository https://github.com/OWNER/RELEASES -PublicKeyFile C:\keys\echo.pub -SecretKeyFile C:\keys\echo.key
```

发布 `dist/Echo-VERSION-windows-x64-setup.exe`、对应 `.minisig` 与
`dist/update-windows-x64.json` 到相同 `vVERSION` release；Mac 和 Windows 使用同一个发布公钥。
脚本只打包和签名，不自动上传。没有发布配置的 CI 测试包保持更新不可用。
Minisign 验证更新包来源，不代替 Authenticode，不消除 SmartScreen 提示。

更新器先验证 Minisign 和版本，将 helper 拷贝到临时目录，验证父进程就是目标安装，
通知应用执行保存屏障后才等待应用退出，再运行静默安装并重启。
安装失败保留原始日志并停止，不切换其他安装方式。成功后清理下载的安装包；
Windows 无法删除仍在运行的 helper，因此其可执行文件与诊断日志留在
`%TEMP%\echo-update-*`，可在进程退出后清理。安装目录必须是本地盘，不能是网络共享。

## Echo 官方发布

正式更新源是 `https://github.com/JOYCEQL/Echo`。首次安装使用 README 中的 macOS Apple Silicon ZIP 或 Windows x64 安装程序。仓库中的 `packaging/echo-update.pub` 是验证两个平台更新包的公钥。

macOS 和 Windows 工作流会在官方仓库的 main 或版本 tag 构建中嵌入同一公钥和更新源。Windows 工作流还会测试安装与卸载。发布时从与发布提交 SHA 一致的成功运行中下载两个平台的产物，在发行机器上签名。Minisign 的 trusted comment 使用对应 ZIP 或安装程序的完整文件名。将两个平台的安装包、签名和包含版本、平台、字节数及完整签名的更新清单一起上传至同一个草稿。核对签名和清单后再公开，并以匿名下载验证 README 的两个链接和 latest 更新清单。
