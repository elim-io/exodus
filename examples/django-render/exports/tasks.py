import csv
from datetime import date

from celery import shared_task
from django.conf import settings

from .models import Entry


@shared_task
def nightly_export():
    path = settings.MEDIA_ROOT / f"export-{date.today()}.csv"
    with open(path, "w", newline="") as f:
        writer = csv.writer(f)
        for e in Entry.objects.order_by("id"):
            writer.writerow([e.id, e.day, e.amount, e.memo])
