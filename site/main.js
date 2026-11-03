// OpenPaper website: language switch (EN, KO, JA, ZH), the visitor's OS as the filled download
// button, and copy buttons. English lives in index.html; the other languages replace it.

const STRINGS = {
  ko: {
    title: 'OpenPaper · 어떤 논문이든 내 언어로',
    eyebrow: '오픈소스 · macOS와 Windows',
    h1: '어떤 논문이든 내 언어로 읽으세요.',
    lead1: 'arXiv에서 논문을 찾고, 원문과 번역을 나란히 놓고 읽고, 논문에 대해 질문하세요.',
    lead2: '이미 로그인한 Claude Code나 Codex로 동작하고, 원하면 API 키도 쓸 수 있습니다. 논문은 내 컴퓨터에만 저장됩니다.',
    mac: 'macOS용 다운로드',
    macSub: 'Apple silicon, Intel · macOS 11 이상',
    win: 'Windows용 다운로드',
    winSub: 'Windows 10, 11 · 64비트',
    fine: 'MIT 라이선스로 무료입니다.',
    fH1: '원문과 번역을 함께.',
    fH2: '문장 단위로.',
    c1t: '이어지는 문장',
    c1p: '문장을 가리키면 다른 쪽 열의 짝 문장이 함께 밝아집니다. 수식, 그림, 표는 논문의 원래 자리에 그대로 있습니다.',
    c2t: '질문과 요약',
    c2p: '구절을 선택해 질문하거나, 논문 전체를 요약할 수 있습니다. 답변은 내 언어로 나옵니다.',
    c3t: '논문 찾기',
    c3p: '키워드로 arXiv를 검색하거나, arXiv ID나 URL을 붙여 넣거나, PDF를 여세요. AI 찾기는 arXiv에 실제로 있는 논문만 추천합니다.',
    c4t: '7개 언어, 2가지 방식',
    c4p: '한국어, 일본어, 중국어, 스페인어, 프랑스어, 독일어, 포르투갈어로 번역합니다. 충실한 번역과, 어려운 용어를 쉬운 말로 풀어 주는 쉬운 버전 중에 고를 수 있습니다.',
    c4a: '논문, 번역, 요약은 내 컴퓨터에 일반 파일로 저장됩니다.',
    c4b: 'Claude Code, Codex, Anthropic API, OpenRouter 같은 OpenAI 호환 API 중에서 모델을 고를 수 있습니다.',
    c4c: '읽던 곳에서 다시 열리고, 양쪽 열에서 글자를 찾을 수 있습니다.',
    iH: '시작하기',
    i1t: 'macOS: 명령 한 줄',
    i1p: '앱을 설치하고, Claude Code가 없으면 설치한 뒤 로그인을 시작합니다. 다시 실행하면 업데이트됩니다.',
    i2t: 'Windows: 설치 프로그램',
    i2p: '설치 프로그램을 내려받아 실행하세요. 관리자 권한 없이 내 계정에만 설치됩니다. Claude Code가 아직 없다면 PowerShell에서 아래 명령을 실행하고 로그인하세요.',
    i3t: '소스에서 빌드',
    i3p: 'Tauri 2, Rust, React로 만들었습니다. Rust, Node, pnpm과 로그인된 Claude Code가 필요합니다.',
    copy: '복사',
    copied: '복사됨',
    ctaH: 'Sionic AI가 공개적으로 만듭니다',
    ctaP: 'OpenPaper는 오픈소스입니다. 문제를 알려 주거나, 기능을 제안하거나, pull request를 보내 주세요.',
    ctaGh: 'GitHub에서 보기',
    ctaRel: '모든 릴리스',
    ctaDocs: '동작 방식',
    mit: 'MIT 라이선스',
  },
  ja: {
    title: 'OpenPaper · どんな論文も自分の言語で',
    eyebrow: 'オープンソース · macOS と Windows',
    h1: 'どんな論文も、自分の言語で。',
    lead1: 'arXiv で論文を探し、原文と翻訳を並べて読み、論文について質問できます。',
    lead2: 'ログイン済みの Claude Code や Codex で動き、API キーも使えます。論文はあなたのコンピューターだけに保存されます。',
    mac: 'macOS 版をダウンロード',
    macSub: 'Apple silicon と Intel · macOS 11 以降',
    win: 'Windows 版をダウンロード',
    winSub: 'Windows 10 と 11 · 64 ビット',
    fine: 'MIT ライセンスで無料です。',
    fH1: '原文と翻訳を、いっしょに。',
    fH2: '一文ずつ。',
    c1t: 'つながる文',
    c1p: '文を指すと、反対側の列の対応する文も光ります。数式、図、表は論文の元の位置のままです。',
    c2t: '質問と要約',
    c2p: '一節を選んで質問したり、論文全体を要約したりできます。回答はあなたの言語で返ってきます。',
    c3t: '論文を探す',
    c3p: 'キーワードで arXiv を検索するか、arXiv の ID や URL を貼り付けるか、PDF を開きます。AI 検索は arXiv に実在する論文だけを提案します。',
    c4t: '7 つの言語、2 つのスタイル',
    c4p: '韓国語、日本語、中国語、スペイン語、フランス語、ドイツ語、ポルトガル語に翻訳します。忠実な翻訳か、難しい用語をやさしい言葉で説明するやさしい版を選べます。',
    c4a: '論文、翻訳、要約は、あなたのコンピューターに普通のファイルとして保存されます。',
    c4b: 'Claude Code、Codex、Anthropic API、OpenRouter などの OpenAI 互換 API からモデルを選べます。',
    c4c: '読みかけの場所から再開でき、両方の列で文字を検索できます。',
    iH: 'はじめる',
    i1t: 'macOS: コマンド 1 行',
    i1p: 'アプリをインストールし、Claude Code がなければインストールしてログインを始めます。もう一度実行すると更新されます。',
    i2t: 'Windows: セットアップ',
    i2p: 'セットアッププログラムをダウンロードして実行します。管理者権限なしで、あなたのユーザーにだけインストールされます。Claude Code がまだない場合は、PowerShell で次を実行してログインしてください。',
    i3t: 'ソースからビルド',
    i3p: 'Tauri 2、Rust、React で作られています。Rust、Node、pnpm と、ログイン済みの Claude Code が必要です。',
    copy: 'コピー',
    copied: 'コピーしました',
    ctaH: 'Sionic AI がオープンに開発しています',
    ctaP: 'OpenPaper はオープンソースです。問題の報告、機能の提案、pull request をお待ちしています。',
    ctaGh: 'GitHub で見る',
    ctaRel: 'すべてのリリース',
    ctaDocs: '仕組み',
    mit: 'MIT ライセンス',
  },
  zh: {
    title: 'OpenPaper · 用你的语言读任何论文',
    eyebrow: '开源 · macOS 和 Windows',
    h1: '用你的语言，读任何论文。',
    lead1: '在 arXiv 上找论文，原文与译文并排阅读，还能就论文提问。',
    lead2: '它使用你已登录的 Claude Code 或 Codex 运行，也可以使用 API 密钥。论文只保存在你的电脑上。',
    mac: '下载 macOS 版',
    macSub: 'Apple silicon 和 Intel · macOS 11 及以上',
    win: '下载 Windows 版',
    winSub: 'Windows 10 和 11 · 64 位',
    fine: '基于 MIT 许可证免费使用。',
    fH1: '原文与译文，放在一起。',
    fH2: '逐句对照。',
    c1t: '句子联动',
    c1p: '指向一句话，另一栏中对应的句子也会一起高亮。公式、图和表都保留在论文原来的位置。',
    c2t: '提问与总结',
    c2p: '选中一段内容提问，或总结整篇论文。回答使用你的语言。',
    c3t: '查找论文',
    c3p: '用关键词搜索 arXiv，粘贴 arXiv ID 或链接，或打开 PDF。AI 查找只推荐 arXiv 上真实存在的论文。',
    c4t: '7 种语言，2 种风格',
    c4p: '可译成韩语、日语、中文、西班牙语、法语、德语或葡萄牙语。可以选择忠实翻译，或用浅显的话解释难懂术语的简明版。',
    c4a: '论文、译文和总结都以普通文件的形式保存在你的电脑上。',
    c4b: '可以在 Claude Code、Codex、Anthropic API 以及 OpenRouter 等 OpenAI 兼容 API 之间选择模型。',
    c4c: '重新打开论文时回到上次读到的位置，还能在两栏中查找文字。',
    iH: '开始使用',
    i1t: 'macOS：一条命令',
    i1p: '安装应用；如果没有 Claude Code，会先安装再开始登录。再次运行即可更新。',
    i2t: 'Windows：安装程序',
    i2p: '下载并运行安装程序。它只为当前用户安装，不需要管理员权限。如果还没有 Claude Code，请在 PowerShell 中运行下面的命令并登录。',
    i3t: '从源码构建',
    i3p: '基于 Tauri 2、Rust 和 React。需要 Rust、Node、pnpm 以及已登录的 Claude Code。',
    copy: '复制',
    copied: '已复制',
    ctaH: '由 Sionic AI 公开开发',
    ctaP: 'OpenPaper 是开源项目。欢迎报告问题、提出功能建议或提交 pull request。',
    ctaGh: '在 GitHub 上查看',
    ctaRel: '全部版本',
    ctaDocs: '工作原理',
    mit: 'MIT 许可证',
  },
}
const HTML_LANG = { en: 'en', ko: 'ko', ja: 'ja', zh: 'zh-Hans' }
const KEY = 'openpaper.site.lang'

// English is whatever index.html says; keep it so switching back restores it.
const english = { title: document.title, copy: 'Copy', copied: 'Copied' }
document.querySelectorAll('[data-i18n]').forEach(el => {
  english[el.dataset.i18n] ??= el.textContent
})
const text = (lang, key) => (lang !== 'en' && STRINGS[lang]?.[key]) || english[key]

function pickLang() {
  const asked = new URLSearchParams(location.search).get('lang')
  if (asked && HTML_LANG[asked]) return asked
  try {
    const saved = localStorage.getItem(KEY)
    if (saved && HTML_LANG[saved]) return saved
  } catch {}
  const nav = (navigator.languages?.[0] || navigator.language || 'en').toLowerCase()
  return Object.keys(HTML_LANG).find(l => nav.startsWith(l)) || 'en'
}

let current = 'en'
function setLang(lang, save) {
  current = lang
  document.documentElement.lang = HTML_LANG[lang]
  document.title = text(lang, 'title')
  document.querySelectorAll('[data-i18n]').forEach(el => {
    el.textContent = text(lang, el.dataset.i18n)
  })
  document.querySelectorAll('.langs button').forEach(b => b.setAttribute('aria-pressed', String(b.dataset.lang === lang)))
  if (save) {
    try {
      localStorage.setItem(KEY, lang)
    } catch {}
  }
}

document.querySelectorAll('.langs button').forEach(b => b.addEventListener('click', () => setLang(b.dataset.lang, true)))
setLang(pickLang(), false)

// The visitor's platform first and filled; macOS when it can't be told.
const isWindows = /Windows/i.test(navigator.userAgent)
const primary = document.querySelector(`.dl[data-os="${isWindows ? 'win' : 'mac'}"]`)
primary.classList.add('primary')
primary.parentElement.prepend(primary)

document.querySelectorAll('.copy').forEach(btn => {
  btn.addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(btn.dataset.copy)
    } catch {
      return
    }
    btn.textContent = text(current, 'copied')
    btn.classList.add('done')
    setTimeout(() => {
      btn.textContent = text(current, 'copy')
      btn.classList.remove('done')
    }, 1600)
  })
})

// Until the latest release carries the Windows installer (releases before v0.2.3 don't), send
// the Windows button to the releases page instead of a missing file. Show the latest version.
fetch('https://api.github.com/repos/sionic-ai/openpaper/releases/latest', { headers: { Accept: 'application/vnd.github+json' } })
  .then(r => (r.ok ? r.json() : null))
  .then(rel => {
    if (!rel) return
    const names = new Set((rel.assets || []).map(a => a.name))
    if (!names.has('OpenPaper-windows-x64-setup.exe')) {
      document.querySelector('.dl[data-os="win"]').href = 'https://github.com/sionic-ai/openpaper/releases'
    }
    if (!names.has('OpenPaper-macos-universal.zip')) {
      document.querySelector('.dl[data-os="mac"]').href = 'https://github.com/sionic-ai/openpaper/releases'
    }
    const v = document.querySelector('.version')
    if (v && /^v\d/.test(rel.tag_name)) {
      v.textContent = rel.tag_name
      v.hidden = false
    }
  })
  .catch(() => {})

// Motion: sections rise into place as they scroll into view, the hero screenshot stands up from a
// tilt as it comes up the screen, and the nav gets a hairline once the page scrolls.
const still = matchMedia('(prefers-reduced-motion: reduce)').matches
const revealed = document.querySelectorAll('[data-reveal]')
if (still || !('IntersectionObserver' in window)) {
  revealed.forEach(el => el.classList.add('in'))
} else {
  const seen = new IntersectionObserver(
    entries => {
      for (const e of entries) {
        if (!e.isIntersecting) continue
        e.target.classList.add('in')
        seen.unobserve(e.target)
      }
    },
    { rootMargin: '0px 0px -8% 0px', threshold: 0.12 }
  )
  revealed.forEach(el => seen.observe(el))
}

// The background drifts through soft mint, pale sky and sage as the page scrolls. Each stop is the
// colors of the four fields (--a to --d) and the page (--base) at that point of the page.
const AMBIENT = [
  ['#bfe6e0', '#d9e2e1', '#cdeee9', '#e4ebef', '#ffffff'],
  ['#cde2ee', '#c4e8e1', '#e2e7ec', '#d6ece6', '#fbfdfd'],
  ['#dbe5df', '#cbe2ee', '#c3e5de', '#e9e6dc', '#f9fbfa'],
  ['#b4dfd8', '#d2dfe9', '#a9d8cf', '#dbe9e4', '#f5faf9'],
]
const ambient = document.querySelector('.ambient')
const root = document.documentElement.style
const rgb = hex => [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16))
const AMBIENT_RGB = AMBIENT.map(stop => stop.map(rgb))
function paintAmbient(s) {
  const x = s * (AMBIENT_RGB.length - 1)
  const i = Math.min(AMBIENT_RGB.length - 2, Math.floor(x))
  const t = x - i
  const from = AMBIENT_RGB[i]
  const to = AMBIENT_RGB[i + 1]
  ;['--a', '--b', '--c', '--d', '--base'].forEach((name, k) => {
    const c = from[k].map((v, j) => Math.round(v + (to[k][j] - v) * t))
    root.setProperty(name, `rgb(${c.join(' ')})`)
  })
  if (!still) ambient.style.setProperty('--s', s.toFixed(3))
}

const nav = document.querySelector('.nav')
const shot = document.querySelector('.hero-shot')
let frame = 0
function onScroll() {
  frame = 0
  nav.classList.toggle('scrolled', scrollY > 8)
  const room = document.documentElement.scrollHeight - innerHeight
  paintAmbient(room > 0 ? Math.min(1, Math.max(0, scrollY / room)) : 0)
  if (still || !shot) return
  // 0 while the screenshot's top sits at the bottom of the screen, 1 once it reaches 25% from the top.
  const top = shot.parentElement.getBoundingClientRect().top
  const p = Math.min(1, Math.max(0, (innerHeight - top) / (innerHeight * 0.75)))
  shot.style.setProperty('--p', p.toFixed(3))
}
addEventListener('scroll', () => { if (!frame) frame = requestAnimationFrame(onScroll) }, { passive: true })
addEventListener('resize', onScroll)
onScroll()
