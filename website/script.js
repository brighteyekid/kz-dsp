gsap.registerPlugin(ScrollTrigger);

// Initialize Lenis
const lenis = new Lenis({
    lerp: 0.15, 
    smoothWheel: true,
});

lenis.on('scroll', ScrollTrigger.update);

function raf(time) {
  lenis.raf(time);
  requestAnimationFrame(raf);
}
requestAnimationFrame(raf);

// Wait for load
window.addEventListener("load", () => {
    const loader = document.querySelector('.loader');
    
    // Hide loader with GSAP
    gsap.to(loader, {
        opacity: 0,
        duration: 0.8,
        delay: 0.5,
        ease: "power2.inOut",
        onComplete: () => {
            loader.style.display = 'none';
        }
    });

    // Animate everything in one go sequentially
    gsap.from(".reveal, .feature-card, .arch-container, .b-row", {
        y: 40,
        opacity: 0,
        duration: 1.2,
        stagger: 0.1,
        ease: "power3.out",
        delay: 0.8
    });
    
    // Benchmark bars expanding
    gsap.utils.toArray('.b-bar').forEach(bar => {
        gsap.to(bar, {
            width: bar.getAttribute('data-width'),
            duration: 1.5,
            delay: 1.5,
            ease: "expo.out"
        });
    });
});

// Interactive Canvas Tile Background
const canvas = document.getElementById('bg-canvas');
const ctx = canvas.getContext('2d');

let width, height;
const tileSize = 60;
let tiles = [];
let mouse = { x: -100, y: -100 };

function resizeCanvas() {
    width = window.innerWidth;
    height = window.innerHeight;
    canvas.width = width;
    canvas.height = height;
    
    tiles = [];
    for (let y = 0; y < height; y += tileSize) {
        for (let x = 0; x < width; x += tileSize) {
            tiles.push({ x, y, alpha: 0 });
        }
    }
}
resizeCanvas();
window.addEventListener('resize', resizeCanvas);

window.addEventListener('mousemove', (e) => {
    mouse.x = e.clientX;
    mouse.y = e.clientY;
});

function drawGrid() {
    ctx.clearRect(0, 0, width, height);
    
    tiles.forEach(tile => {
        // Distance from mouse to tile center
        const dx = mouse.x - (tile.x + tileSize / 2);
        const dy = mouse.y - (tile.y + tileSize / 2);
        const dist = Math.sqrt(dx * dx + dy * dy);
        
        // If close to mouse, light up
        if (dist < 120) {
            tile.alpha = 0.15; // Max opacity for the hover effect
        } else {
            // Fade out
            tile.alpha = Math.max(0, tile.alpha - 0.003);
        }
        
        // Draw Fill
        if (tile.alpha > 0) {
            ctx.fillStyle = `rgba(26, 168, 142, ${tile.alpha})`;
            ctx.fillRect(tile.x, tile.y, tileSize, tileSize);
        }
        
        // Draw Grid Lines
        ctx.strokeStyle = 'rgba(255, 255, 255, 0.03)';
        ctx.lineWidth = 1;
        ctx.strokeRect(tile.x, tile.y, tileSize, tileSize);
    });
    
    requestAnimationFrame(drawGrid);
}
drawGrid();
