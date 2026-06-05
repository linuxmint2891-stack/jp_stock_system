import os
import sys
import requests
from dotenv import load_dotenv

def send_discord_notification(title, message):
    # .env を読み込む (プロジェクトルートまたはスクリプトディレクトリの親を想定)
    load_dotenv(os.path.join(os.path.dirname(__file__), '..', '.env'))
    webhook_url = os.getenv("DISCORD_WEBHOOK_URL")
    
    if not webhook_url:
        print("⚠️ DISCORD_WEBHOOK_URL not found in .env")
        return

    payload = {
        "embeds": [
            {
                "title": title,
                "description": message,
                "color": 3066993 # Green-ish
            }
        ]
    }

    try:
        response = requests.post(webhook_url, json=payload)
        response.raise_for_status()
        print("✅ Discord notification sent.")
    except Exception as e:
        print(f"❌ Failed to send Discord notification: {e}")

if __name__ == "__main__":
    if len(sys.argv) < 3:
        # デバッグ用
        # send_discord_notification("Debug Title", "Debug Message")
        pass
    else:
        send_discord_notification(sys.argv[1], sys.argv[2])
