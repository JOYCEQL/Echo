# Echo 发布与应用内更新

Echo 的下载域名是 `https://echo-downloads.magicv.art`。Cloudflare R2 Standard 存储桶为 `echo-downloads`，位于亚太地区。缓存规则只匹配该域名的版本目录和 `latest/`。GitHub Release 自动归档相同产物，客户端只访问下载域名。

| 公开路径 | 内容 | Cache-Control |
|---|---|---|
| `v<版本>/` | 各架构的安装包、更新包、源码、独立签名、SHA-256、汇总清单及其签名 | 一年，immutable |
| `latest/Echo-macos-arm64.dmg`、`latest/Echo-macos-x86_64.dmg` | 首次安装 Mac 应用 | 五分钟 |
| `latest/Echo-windows-x64-setup.exe` | Windows 安装及更新使用的 Inno Setup 程序 | 五分钟 |
| `latest/update-<platform>.json` | 版本、平台、固定包地址、包长度和 Minisign 签名 | 五分钟 |
| `latest/policy.json` | 包含签名的最低版本策略 | 五分钟 |
| `latest/Echo-source.tar.gz`、`latest/echo.rb` | 对应源码和 Homebrew cask；cask 在两种 Mac 架构均发布时生成 | 五分钟 |

## 从标签自动发版

在 GitHub Actions 的 `Prepare release` 中选择 `patch`、`minor` 或 `major`。它会在 `main` 创建版本提交和标签，并调用 `Publish release`。也可以本地用 `python3 scripts/bump_version.py --part patch` 同步版本，将提交和匹配的 `vX.Y.Z` 标签推送到 GitHub。普通分支推送只运行构建测试。

发布工作流验证标签与源码版本及 main 历史，测试并构建 Apple Silicon、Intel Mac 和 Windows x64。随后签名并上传 R2，验证公开下载地址，再核对 GitHub 草稿全部附件的 SHA-256 并正式发布。发布失败时，在同一次运行中选择 **Re-run failed jobs**，复用已构建的包。

GitHub Environment `release` 仅允许 `v*` 标签部署。它使用 `R2_ACCESS_KEY_ID`、`R2_SECRET_ACCESS_KEY`、`MINISIGN_KEY`（加密密钥文件）和 `MINISIGN_PASSWORD`。R2 凭据仅授权发行桶的对象读写。

## 生成平台包

- Mac 运行 `bash scripts/release.sh aarch64-apple-darwin` 和 `bash scripts/release.sh x86_64-apple-darwin`。跨架构时提前用 rustup 安装对应目标。每个架构生成供手动安装的 DMG 和供更新的 ZIP。若已配置 Developer ID 和公证服务，同时设置 `APPLE_SIGNING_IDENTITY` 与 `APPLE_NOTARY_PROFILE`，脚本会签名、公证、装订并校验。缺少这两项时，构建产物使用 ad-hoc 签名。
- Windows 安装 Rust MSVC 工具链与 Inno Setup 后，运行 `pwsh -File scripts/release-windows.ps1`。正式系统代码签名使用 `ECHO_WINDOWS_CERT_THUMBPRINT` 和 `ECHO_WINDOWS_TIMESTAMP_URL`。提供两项后脚本会对应用和安装程序签名并验证。便携 ZIP 用于测试，应用内更新使用安装版。
- 两端构建都把 `packaging/echo-update.pub` 嵌入客户端。妥善备份匹配的加密 Minisign 私钥和解锁凭据。Minisign 用于发行包认证，Apple 和 Windows 系统代码签名需要各自的证书。

## 发布

将两份 DMG、两份 Mac ZIP 和 Windows 安装程序放到 `dist/`。使用 Python 3.11+ 运行 `scripts/package_source.py`，生成带有 vendor、构建脚本、资源与许可证的对应源码归档。工作区内有新的源码文件时，逐一审阅并用 `--include 路径` 指定。

发布脚本通过 S3 接口写入指定的 R2 桶。给发行机配置仅对该桶有对象读写权限的凭据。以下命令适用于本机的发行身份。

```sh
export AWS_SHARED_CREDENTIALS_FILE="$HOME/.config/echo-release/r2.credentials"
export AWS_PROFILE=echo-r2
export MINISIGN_SECRET_KEY="$HOME/.config/echo-release/r2-release.key"
export ECHO_MINISIGN_KEYCHAIN_SERVICE=art.magicv.echo.r2-release
uv run --script scripts/publish_downloads.py release macos-arm64 macos-x86_64 windows-x64
```

脚本按完整文件名签名并用仓库公钥反验，先上传版本目录，逐个从 R2 读回比对，再更新固定链接，最后更新平台清单。版本目录采用条件写入。同名文件一旦发布，只允许重传完全一致的内容。发布锁串行化多个发行机器的写入。若进程被强制终止，在确认没有活动发布后移除桶根的 `_publish.lock`，用相同产物重跑。构建有变化时必须递增版本。各平台清单分别生效。

公网验收命令为 `python3 scripts/verify_downloads.py X.Y.Z macos-arm64 macos-x86_64 windows-x64`，会从自定义域名匿名下载并逐项验证。

手工恢复 GitHub 归档时，检出对应的发行标签并运行 `python3 scripts/publish_github.py vX.Y.Z`。Homebrew tap 的 `Casks/echo.rb` 可直接使用版本目录中生成的 cask。

## 日常更新

安装版启动 10 秒后检查，每 12 小时再次检查。回到前台且距上次检查已超过一小时也会检查。设置可关闭自动检查，仍可手动检查。发现新版本后由用户点击下载。更新器按平台和版本推导固定 HTTPS 地址、核对包长、验证 Minisign 签名及文件名，只接受比当前版本更新的包。系统代理支持 macOS 静态代理及 Windows Internet 设置；环境变量按 curl 的代理规则优先。若文件大小或签名不符，可用新查询串向同一域名重新获取一次。

验证完成后等待用户确认重启，结束编辑并等待笔记写入磁盘。保存失败会取消安装。Mac 核对 Bundle ID、架构及版本、校验代码签名，已带开发团队签名的安装版还要求团队一致；辅助进程在同卷原子交换应用后重启。Windows 安装版经辅助进程再次验证、静默安装并重新启动。系统首次安装时显示的安全提示取决于系统代码签名和公证。

## 紧急更新

先发布修复版本，再发布策略。以下示例仅对指定 Windows 版本生效。

```sh
uv run --script scripts/publish_downloads.py policy \
  --platform-min windows-x64=0.0.1 \
  --message-zh '此版本修复了保存问题，请重启更新。' \
  --message-en 'Restart to install the save fix.'
```

`--min-version` 设置所有平台的最低版本，`--platform-min` 可多次指定覆盖值。空的 `policy` 命令签发清除策略。客户端使用独立的签名标识验证策略，启动三秒后检查，此后每小时检查一次，独立于日常更新开关。最近的有效策略缓存在应用数据目录。低于最低版本的客户端自动下载并验签，下载失败时仍可编辑，十分钟后重试。包就绪后弹窗提供重启更新按钮。
