#!/bin/bash

# このスクリプトが存在するフォルダに移動する（ダブルクリック対策）
cd "$(dirname "$0")"

# エラーが発生したらその時点でスクリプトを終了させる
set -e

# 1. すべての変更をステージング領域に追加
echo "📦 変更を追加中..."
git add .

# 2. コマンド引数があればそれをコミットメッセージにし、なければデフォルト値を使用
COMMIT_MSG="${1:-"Auto commit by script"}"

# 3. コミットを実行
echo "💾 コミット中: '${COMMIT_MSG}'"
git commit -m "$COMMIT_MSG"

# 4. 現在のブランチ名を取得してGitHubにプッシュ
CURRENT_BRANCH=$(git branch --show-current)
echo "🚀 GitHubへプッシュ中 (${CURRENT_BRANCH})..."
git push origin "$CURRENT_BRANCH"

echo "✨ 完了しました！"