import os

from celery import Celery
from celery.schedules import crontab

os.environ.setdefault("DJANGO_SETTINGS_MODULE", "books.settings")

app = Celery("books", broker=os.environ["CELERY_BROKER_URL"])
app.autodiscover_tasks()

app.conf.beat_schedule = {
    "nightly-export": {
        "task": "exports.tasks.nightly_export",
        "schedule": crontab(hour=2, minute=0),
    },
}
