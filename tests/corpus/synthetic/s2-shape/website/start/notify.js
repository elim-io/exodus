const form = document.getElementById('notify');

form.addEventListener('submit', async (event) => {
  event.preventDefault();
  const email = new FormData(form).get('email');
  await fetch('https://europe-west1-tiles-example.cloudfunctions.net/signup', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ email }),
  });
  form.replaceWith('Thanks, we will write.');
});
