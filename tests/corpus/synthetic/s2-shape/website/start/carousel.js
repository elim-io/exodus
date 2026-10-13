const slides = document.querySelectorAll('.carousel img');
let current = 0;

function show(i) {
  slides.forEach((s, n) => s.classList.toggle('on', n === i));
}

show(current);
setInterval(() => {
  current = (current + 1) % slides.length;
  show(current);
}, 5000);
