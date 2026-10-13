from django.db import models


class Entry(models.Model):
    day = models.DateField()
    amount = models.DecimalField(max_digits=12, decimal_places=2)
    memo = models.CharField(max_length=200)
