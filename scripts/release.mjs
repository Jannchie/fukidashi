// 发版：改版本号、提交、打 tag。推上 GitHub 后由 .github/workflows/release.yml 构建并发布。
//
//   pnpm release patch | minor | major | 1.2.3
//   pnpm release 1.2.3 --push      # 顺便推送、等构建完成后发布（约 15 分钟）
import { execFileSync, execSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'

const [arg, ...flags] = process.argv.slice(2)
if (!arg) {
  console.error('用法：pnpm release <patch|minor|major|x.y.z> [--push]')
  process.exit(1)
}

const sh = (cmd) => execSync(cmd, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] }).trim()

if (sh('git status --porcelain')) {
  console.error('工作区有未提交的改动，先提交或暂存再发版')
  process.exit(1)
}

const pkg = JSON.parse(readFileSync('package.json', 'utf8'))
const [ma, mi, pa] = pkg.version.split('.').map(Number)
const version = {
  major: `${ma + 1}.0.0`,
  minor: `${ma}.${mi + 1}.0`,
  patch: `${ma}.${mi}.${pa + 1}`,
}[arg] ?? arg
if (!/^\d+\.\d+\.\d+$/.test(version)) {
  console.error(`版本号不对：${version}`)
  process.exit(1)
}

// 三处版本号必须一致：安装包和更新器读 tauri.conf.json，Cargo 和 npm 只是跟着走
const edit = (file, from, to) => {
  const s = readFileSync(file, 'utf8')
  const t = s.replace(from, to)
  if (s === t) throw new Error(`${file} 里没找到版本号`)
  writeFileSync(file, t)
}
edit('package.json', /"version": "[^"]+"/, `"version": "${version}"`)
edit('src-tauri/tauri.conf.json', /"version": "[^"]+"/, `"version": "${version}"`)
edit('src-tauri/Cargo.toml', /^version = "[^"]+"/m, `version = "${version}"`)
edit('src-tauri/Cargo.lock', /(name = "fukidashi"\r?\nversion = )"[^"]+"/, `$1"${version}"`)

sh('git add package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock')
sh(`git commit -m "release: v${version}"`)
sh(`git tag v${version}`)
console.log(`已提交并打上 v${version}`)

// Release 由本机的 gh 账号创建，工作流只负责上传构建产物：
// 工作流的 GITHUB_TOKEN 创建 Release 时会被拒绝（Resource not accessible by integration），上传则不受影响。
//
// 先建成预发布：应用读的是「最新 Release」里的 latest.json，构建要十几分钟，
// 这期间新 Release 还没有 latest.json，标成最新会让所有人检查更新都失败。
// 等工作流传完再转正。
const notes = `下载并运行 \`Fukidashi_${version}_x64-setup.exe\`。已安装旧版本的用户会在应用内收到更新提示。`
const releaseArgs = ['release', 'create', `v${version}`, '--verify-tag', '--prerelease', '--title', `Fukidashi v${version}`, '--notes', notes]

/** 等这个 tag 触发的构建跑完，成功了就把 Release 转为正式的最新版 */
function publishWhenBuilt() {
  const gh = (args) => execFileSync('gh', args, { encoding: 'utf8' }).trim()
  let run = ''
  for (let i = 0; i < 30 && !run; i++) {
    run = gh(['run', 'list', '--branch', `v${version}`, '--limit', '1', '--json', 'databaseId', '-q', '.[0].databaseId'])
    if (!run) execSync('node -e "setTimeout(() => {}, 5000)"')
  }
  if (!run) throw new Error('没找到这个 tag 的构建')
  console.log(`等待构建 ${run} 完成……`)
  try {
    execFileSync('gh', ['run', 'watch', run, '--exit-status', '--interval', '30'], { stdio: 'ignore' })
  } catch {
    console.error(`构建失败，Release 保持预发布。修好后重跑构建，再执行：gh release edit v${version} --prerelease=false --latest`)
    process.exit(1)
  }
  gh(['release', 'edit', `v${version}`, '--prerelease=false', '--latest'])
  console.log(`v${version} 已发布为最新版本`)
}

if (flags.includes('--push')) {
  sh('git push')
  sh(`git push origin v${version}`)
  execFileSync('gh', releaseArgs, { stdio: 'inherit' })
  publishWhenBuilt()
} else {
  console.log(`推送 tag 后用 gh 创建 Release，或直接重新运行：pnpm release ${version} --push`)
}
