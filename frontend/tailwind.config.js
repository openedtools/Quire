/** A token is an "R G B" triplet, so opacity modifiers such as bg-paper/80 work. */
const token = (name) => `rgb(var(--${name}) / <alpha-value>)`;

/** @type {import('tailwindcss').Config} */
module.exports = {
    darkMode: ['class'],
    content: [
    './src/pages/**/*.{js,ts,jsx,tsx,mdx}',
    './src/components/**/*.{js,ts,jsx,tsx,mdx}',
    './src/app/**/*.{js,ts,jsx,tsx,mdx}',
    // Class names also live in lib (speaker colours), hooks and contexts
    './src/lib/**/*.{js,ts,jsx,tsx}',
    './src/hooks/**/*.{js,ts,jsx,tsx}',
    './src/contexts/**/*.{js,ts,jsx,tsx}',
  ],
  theme: {
  	extend: {
  		fontFamily: {
  			sans: ['var(--font-sans)'],
  			serif: ['var(--font-serif)']
  		},
  		// Semantic colours from src/styles/tokens.css. Components use only these (spec F-UI-1).
  		colors: {
  			paper: token('paper'),
  			surface: { DEFAULT: token('surface'), hover: token('surface-hover') },
  			line: { DEFAULT: token('line'), strong: token('line-strong') },
  			ink: { DEFAULT: token('ink'), muted: token('ink-muted') },
  			accent: {
  				DEFAULT: token('accent'),
  				strong: token('accent-strong'),
  				'strong-hover': token('accent-strong-hover'),
  				'strong-pressed': token('accent-strong-pressed'),
  				tint: token('accent-tint')
  			},
  			'on-accent': token('on-accent'),
  			danger: { DEFAULT: token('danger'), tint: token('danger-tint') },
  			success: { DEFAULT: token('success'), tint: token('success-tint') },
  			warning: { DEFAULT: token('warning'), tint: token('warning-tint') },
  			sage: { DEFAULT: token('sage'), tint: token('sage-tint') },
  			disabled: token('disabled'),
  			scrim: 'rgb(var(--scrim) / var(--scrim-alpha))',
  			speaker: Object.fromEntries([1, 2, 3, 4, 5, 6, 7, 8].flatMap((n) => [[n, token(`speaker-${n}`)], [`${n}-ink`, token(`speaker-${n}-ink`)]])),

  			// shadcn names, aliased to the tokens in globals.css
  			background: token('background'),
  			foreground: token('foreground'),
  			border: token('border'),
  			input: token('input'),
  			ring: token('ring'),
  			primary: { DEFAULT: token('primary'), foreground: token('primary-foreground') },
  			secondary: { DEFAULT: token('secondary'), foreground: token('secondary-foreground') },
  			card: { DEFAULT: token('card'), foreground: token('card-foreground') },
  			popover: { DEFAULT: token('popover'), foreground: token('popover-foreground') },
  			muted: { DEFAULT: token('muted'), foreground: token('muted-foreground') },
  			destructive: { DEFAULT: token('destructive'), foreground: token('destructive-foreground') },
  			chart: {
  				'1': token('chart-1'),
  				'2': token('chart-2'),
  				'3': token('chart-3'),
  				'4': token('chart-4'),
  				'5': token('chart-5')
  			}
  		},
  		borderRadius: {
  			card: 'var(--radius-card)',
  			control: 'var(--radius-control)',
  			lg: 'var(--radius-card)',
  			md: 'var(--radius-control)',
  			sm: 'calc(var(--radius-control) - 2px)'
  		},
  		keyframes: {
  			'accordion-down': {
  				from: {
  					height: '0'
  				},
  				to: {
  					height: 'var(--radix-accordion-content-height)'
  				}
  			},
  			'accordion-up': {
  				from: {
  					height: 'var(--radix-accordion-content-height)'
  				},
  				to: {
  					height: '0'
  				}
  			}
  		},
  		animation: {
  			'accordion-down': 'accordion-down 0.2s ease-out',
  			'accordion-up': 'accordion-up 0.2s ease-out'
  		}
  	}
  },
  plugins: [
    require("tailwindcss-animate"),
    require("@tailwindcss/typography"),
    require("@tailwindcss/container-queries"),
  ],
}