export interface PalettePreset {
  name: string;
  colors: string[];
  category: string;
}

export const palettePresets: PalettePreset[] = [
  // 3-Color Palettes
  {
    name: "Minimalist Cool",
    colors: ["#E2E8F0", "#94A3B8", "#0F172A"],
    category: "3-color",
  },
  {
    name: "Warm Contrast",
    colors: ["#FFFBEB", "#F59E0B", "#78350F"],
    category: "3-color",
  },
  {
    name: "Forest Basics",
    colors: ["#ECFDF5", "#10B981", "#064E3B"],
    category: "3-color",
  },

  // 4-Color Palettes
  {
    name: "Cyber Punch",
    colors: ["#120458", "#7A04EB", "#FE75FE", "#FF00A0"],
    category: "4-color",
  },
  {
    name: "Desert Oasis",
    colors: ["#2C363F", "#E75A7C", "#F2F5EA", "#D6EADF"],
    category: "4-color",
  },
  {
    name: "Vintage Card",
    colors: ["#EAE2B7", "#FCBF49", "#F77F00", "#D62828"],
    category: "4-color",
  },

  // 5-Color Palettes
  {
    name: "Velvet Blossom",
    colors: ["#735D78", "#E9BBBE", "#EBA5A2", "#246A73", "#231C36"],
    category: "5-color",
  },
  {
    name: "Lavender Hush",
    colors: ["#9B9E99", "#BBA4BD", "#DBCFE6", "#E9DDEB", "#ECEBF5"],
    category: "5-color",
  },
  {
    name: "City Haven",
    colors: ["#757C8D", "#A68F83", "#AE898F", "#8F6D6E", "#283E4C"],
    category: "5-color",
  },
  {
    name: "Sundown Spark",
    colors: ["#A23438", "#BB5E61", "#D99C97", "#F3C9AF", "#FBE3D0"],
    category: "5-color",
  },
  {
    name: "Coastal Retreat",
    colors: ["#335765", "#74A8A4", "#B6D9E0", "#DBE2DC", "#7F543D"],
    category: "5-color",
  },
  {
    name: "Bay Bloom",
    colors: ["#6F9C9F", "#C0DFD4", "#F0D6B6", "#E08374", "#AC4E5C"],
    category: "5-color",
  },
  {
    name: "Candy Breeze",
    colors: ["#FF8082", "#ED979A", "#DCB3B5", "#B2E1E1", "#A4FDFD"],
    category: "5-color",
  },
  {
    name: "Metropolis Night",
    colors: ["#272727", "#484848", "#FFB900", "#009EFF", "#D7D7D7"],
    category: "5-color",
  },
  {
    name: "Carnival Brights",
    colors: ["#F0932B", "#EB4D4B", "#6AB04C", "#4834D4", "#30336B"],
    category: "5-color",
  },
  {
    name: "Spring Sorbet",
    colors: ["#A29BFE", "#FFEAA7", "#FAB1A0", "#74B9FF", "#55EFC4"],
    category: "5-color",
  },
  {
    name: "Earthy Greens",
    colors: ["#6B705C", "#A5A58D", "#B7B7A4", "#DDBEA9", "#FFE8D6"],
    category: "5-color",
  },
  {
    name: "Spiced Plum",
    colors: ["#320A28", "#511730", "#8E443D", "#E08E45", "#FACFAD"],
    category: "5-color",
  },
  {
    name: "Tropical Harmony",
    colors: ["#023A38", "#06585B", "#B767AE", "#FF7742", "#FFC847"],
    category: "5-color",
  },
  {
    name: "Aqua Serenity",
    colors: ["#026968", "#1B8887", "#3BA7A6", "#74CECD", "#B9E2E2"],
    category: "5-color",
  },
  {
    name: "Retro Soul",
    colors: ["#194A4D", "#318583", "#EBE8DD", "#E3BE68", "#764B4B"],
    category: "5-color",
  },
  {
    name: "Summer Splash (Blue)",
    colors: ["#006C70", "#20929B", "#E6F3E4", "#CFED76", "#356FEA"],
    category: "5-color",
  },
  {
    name: "Summer Lilac",
    colors: ["#0B6568", "#3F959D", "#67BEC2", "#E6C6F8", "#FDE2F8"],
    category: "5-color",
  },
  {
    name: "Tokyo Pop",
    colors: ["#08464A", "#0C757D", "#3A9CA3", "#EEDDEB", "#FC8FD6"],
    category: "5-color",
  },
  {
    name: "Navy Terracotta",
    colors: ["#2C3646", "#BF4C4E", "#EFEADF", "#EDADC4", "#C98F62"],
    category: "5-color",
  },
  {
    name: "Fresh",
    colors: ["#2B1B17", "#5C4033", "#6A7B54", "#7DA1B1", "#D8DCD6", "#F3EFE9"],
    category: "6-color",
  },
  {
    name: "Spiced",
    colors: ["#2B1E1A", "#861D1D", "#BF6F31", "#F4B34C", "#EBCCBA", "#E8E2D1"],
    category: "6-color",
  },
  {
    name: "Frosted",
    colors: ["#2A3439", "#4B5963", "#707C84", "#A9C0CE", "#DDE7ED", "#E6E6E6"],
    category: "6-color",
  },
  {
    name: "Plum Fade",
    colors: ["#150016", "#29104A", "#522C5D", "#845162", "#E3B6B1", "#F7E3DB"],
    category: "6-color",
  },
  {
    name: "Regal",
    colors: ["#11100F", "#5D1C34", "#A67D44", "#899481", "#CDBCAB", "#EFE9E1"],
    category: "6-color",
  },
  {
    name: "Sanddune",
    colors: ["#3A2D28", "#A48374", "#CBAD8D", "#D1C7BD", "#EBE3DB", "#F1EDE6"],
    category: "6-color",
  },
  {
    name: "Dusk",
    colors: ["#3D2321", "#6E3F40", "#9C5B45", "#9BA9B3", "#D0D7E0", "#F1F1F1"],
    category: "6-color",
  },
  {
    name: "Ocean Depths",
    colors: ["#031716", "#032F30", "#0A7075", "#0C969C", "#6BA3BE", "#274D60"],
    category: "6-color",
  },
  {
    name: "Twilight Peach",
    colors: ["#03122F", "#19305C", "#413B61", "#AE7DAC", "#F3DADF", "#F1916D"],
    category: "6-color",
  },
  {
    name: "Crimson Dusk",
    colors: ["#181A2F", "#242E49", "#37415C", "#FDA481", "#B4182D", "#54162B"],
    category: "6-color",
  },
  {
    name: "Dusty Rose & Blue",
    colors: ["#0D1E4C", "#C48CB3", "#E5C9D7", "#83A6CE", "#26415E", "#0B1B32"],
    category: "6-color",
  },
  {
    name: "Sunset Hues",
    colors: ["#1B1931", "#44174E", "#662249", "#A34054", "#ED9E59", "#E98CB9"],
    category: "6-color",
  },
  {
    name: "Soft Lavender & Midnight Navy",
    colors: ["#E1D2F3", "#12112E"],
    category: "2-color",
  },
  {
    name: "Mustard & Nude Pink",
    colors: ["#FBD2C0", "#887114"],
    category: "2-color",
  },
  {
    name: "Deep Maroon & Mint Green",
    colors: ["#50071A", "#AAE8CD"],
    category: "2-color",
  },
  {
    name: "Sky Blue & White",
    colors: ["#FFFFFF", "#7EAFD0"],
    category: "2-color",
  },
  {
    name: "Dark Blue & Blush Pink",
    colors: ["#FFB3C3", "#2A234F"],
    category: "2-color",
  },
  {
    name: "Lavender & Navy Blue",
    colors: ["#EBEBEB", "#10367D"],
    category: "2-color",
  },
  {
    name: "Lime Mist & Forest Pine",
    colors: ["#D6CBC7", "#084734"],
    category: "2-color",
  },
  {
    name: "Crimson Red & Yellow",
    colors: ["#FCF0D6", "#B40023"],
    category: "2-color",
  },
  {
    name: "Deep Teal & Vibrant Orange",
    colors: ["#12363B", "#EF761B"],
    category: "2-color",
  },
  {
    name: "Golden Yellow & Deep Indigo",
    colors: ["#EEB01D", "#353470"],
    category: "2-color",
  },
  {
    name: "Pastel Purple & Forest Green",
    colors: ["#BBBFEC", "#06530B"],
    category: "2-color",
  },
  {
    name: "Ivory & Olive Green",
    colors: ["#FEF9DB", "#574C00"],
    category: "2-color",
  },
  {
    name: "Beige & Orange",
    colors: ["#F0E7D6", "#EA2E00"],
    category: "2-color",
  },
  {
    name: "Vanilla & Teal",
    colors: ["#F3E5C3", "#174E4F"],
    category: "2-color",
  },
  {
    name: "Lime & Evergreen",
    colors: ["#8ED968", "#103C1F"],
    category: "2-color",
  },
  {
    name: "Mint & Mustard",
    colors: ["#D7EAE2", "#4B421B"],
    category: "2-color",
  },
  {
    name: "Burgundy & Sand",
    colors: ["#5C0E14", "#F0E193"],
    category: "2-color",
  },
  {
    name: "Sage & Olive",
    colors: ["#ABC8A2", "#1A2417"],
    category: "2-color",
  },
  {
    name: "Ocean & Sky",
    colors: ["#2772A0", "#CCDDEA"],
    category: "2-color",
  },
  {
    name: "Cherry & Peach",
    colors: ["#E84F5E", "#FCDFC5"],
    category: "2-color",
  },
  {
    name: "Sunset Adventure",
    colors: ["#FE7F2D", "#233D4D"],
    category: "2-color",
  },
  {
    name: "Lavender Magic",
    colors: ["#F492F0", "#5E429C"],
    category: "2-color",
  },
  {
    name: "Electric Lemonade",
    colors: ["#B8FB3C", "#03045E"],
    category: "2-color",
  },
  {
    name: "Wisteria & Cream",
    colors: ["#C69FD5", "#FCFDC8"],
    category: "2-color",
  },
  {
    name: "Crimson Nights",
    colors: ["#D7263D", "#02182B"],
    category: "2-color",
  },
  {
    name: "Vanilla & Moonstone",
    colors: ["#FFEBAF", "#4C9DB0"],
    category: "2-color",
  },
  {
    name: "Claret & Pink",
    colors: ["#670626", "#FFBDC5"],
    category: "2-color",
  },
  {
    name: "Dreamy Ocean",
    colors: ["#EAEBED", "#006989"],
    category: "2-color",
  },
  {
    name: "Ash & Eggshell",
    colors: ["#9D9D8C", "#ECE6D4"],
    category: "2-color",
  },
  {
    name: "Fiery Sunset",
    colors: ["#E74C3C", "#F39C12", "#F1C40F"],
    category: "3-color",
  },
  {
    name: "Neon Jungle",
    colors: ["#50207A", "#FF48B9", "#12CE6A"],
    category: "3-color",
  },
  {
    name: "Rustic Earth",
    colors: ["#414A37", "#99744A", "#DBC2A6"],
    category: "3-color",
  },
  {
    name: "Amethyst Sky",
    colors: ["#50207A", "#D6B9FC", "#838CE5"],
    category: "3-color",
  },
  {
    name: "Nautical Classic",
    colors: ["#10367D", "#EBEBEB", "#74B4D9"],
    category: "3-color",
  },
  {
    name: "Bioluminescent Forest",
    colors: ["#05614B", "#020E0E", "#01DE82"],
    category: "3-color",
  },
  {
    name: "Autumn Harvest",
    colors: ["#FD0D02", "#FEA603", "#4D6802"],
    category: "3-color",
  },
  {
    name: "Crimson Ice",
    colors: ["#0C0E0B", "#D6E7F3", "#D6303A"],
    category: "3-color",
  },
  {
    name: "Hot Pink & Charcoal",
    colors: ["#474747", "#333333", "#FD105E"],
    category: "3-color",
  },
  {
    name: "Lush Greens",
    colors: ["#A4CD01", "#E0D205", "#02754B"],
    category: "3-color",
  },
  {
    name: "Lavender Dreams",
    colors: ["#EAEFFE", "#9787F3", "#2D274B"],
    category: "3-color",
  },
  {
    name: "Classic Pink & Black",
    colors: ["#FFFFFF", "#E281B1", "#050304"],
    category: "3-color",
  },
  {
    name: "Pastel Punch",
    colors: ["#F8DCD3", "#82AAF2", "#FB6A4C"],
    category: "3-color",
  },
  {
    name: "Muted Navy",
    colors: ["#F6F3ED", "#C2CBD3", "#313851"],
    category: "3-color",
  },
  {
    name: "Vibrant Orange",
    colors: ["#FD6500", "#242529", "#FFA300"],
    category: "3-color",
  },
  {
    name: "Magenta Pop",
    colors: ["#FF5402", "#9E0059", "#D6D6D6"],
    category: "3-color",
  },
  {
    name: "Pastel Sky",
    colors: ["#B796FF", "#121710", "#CBFBFD"],
    category: "3-color",
  },
  {
    name: "Coral & Mint",
    colors: ["#FB3F58", "#121619", "#7FD0BD"],
    category: "3-color",
  },
  {
    name: "Citrus Green",
    colors: ["#DCE25A", "#90C52D", "#FFEAB5"],
    category: "3-color",
  },
  {
    name: "Winter Sky",
    colors: ["#2C3E50", "#3498DB", "#ECF0F1", "#95A5A6"],
    category: "4-color",
  },
  {
    name: "Evergreen Forest",
    colors: ["#16A085", "#27AE60", "#2ECC71", "#34495E"],
    category: "4-color",
  },
  {
    name: "Clay & Steel",
    colors: ["#D35400", "#C0392B", "#BDC3C7", "#7F8C8D"],
    category: "4-color",
  },
  {
    name: "Muted Earth",
    colors: ["#232020", "#626C66", "#918B76", "#ACA885", "#E1CA96"],
    category: "5-color",
  },
  {
    name: "Warm Taupe",
    colors: ["#5E3719", "#735238", "#886E58", "#9D8977", "#B2A496"],
    category: "5-color",
  },
  {
    name: "Autumn Forest",
    colors: ["#232020", "#553739", "#955E42", "#9C914F", "#748E54"],
    category: "5-color",
  },
  {
    name: "Golden Shore",
    colors: ["#017D7D", "#509494", "#98C1C1", "#E0E9E9", "#F6C468"],
    category: "5-color",
  },
  {
    name: "Bubble Pop",
    colors: ["#0B585C", "#0B8488", "#DDE8EA", "#DC9EC8", "#E24C4F"],
    category: "5-color",
  },
  {
    name: "Urban Garden",
    colors: ["#094648", "#056362", "#4B844B", "#B4B236", "#DCE2C0"],
    category: "5-color",
  },
  {
    name: "Coral Shore",
    colors: ["#0F595F", "#347377", "#6C989A", "#E7E0DA", "#F4AD8E"],
    category: "5-color",
  },
  {
    name: "Retro Romance",
    colors: ["#2A6F73", "#66908F", "#D6E2E3", "#D3C5C6", "#C3909B"],
    category: "5-color",
  },
  {
    name: "Mountain Lodge",
    colors: ["#0D6663", "#519390", "#A6C0BE", "#DAE6E1", "#7C5D4F"],
    category: "5-color",
  },
  {
    name: "Fresh Joy",
    colors: ["#115A61", "#0F7476", "#3B9495", "#DFEC95", "#CE8FDB"],
    category: "5-color",
  },
  {
    name: "Summer Splash (Pink)",
    colors: ["#126468", "#2B7B83", "#E2896D", "#E6A9B5", "#FDEEAE"],
    category: "5-color",
  },
  {
    name: "Mint Harmony",
    colors: ["#007F7D", "#0E9A98", "#1FD0AF", "#85E7C8", "#C5EDE0"],
    category: "5-color",
  },
  {
    name: "Plum Rose",
    colors: ["#1A141B", "#513953", "#E4DBDE", "#DF94B2", "#DB1C39"],
    category: "5-color",
  },
  {
    name: "Teal & Gold",
    colors: ["#1F2E3B", "#177174", "#A4C9C9", "#E5EBEF", "#B88E22"],
    category: "5-color",
  },
  {
    name: "Sensual",
    colors: ["#1A1110", "#6A0019", "#95897E", "#D9B6B0", "#E1DCD5", "#F7F5F2"],
    category: "6-color",
  },
  {
    name: "Spotify",
    colors: ["#1EDD60", "#1B1716"],
    category: "Popular Brands",
  },
  {
    name: "IKEA",
    colors: ["#0058A3", "#FFDB00"],
    category: "Popular Brands",
  },
  {
    name: "Amazon",
    colors: ["#FF6201", "#171D27"],
    category: "Popular Brands",
  },
  {
    name: "McDonalds",
    colors: ["#DB0007", "#FFBC0D"],
    category: "Popular Brands",
  },
  {
    name: "Netflix",
    colors: ["#B1060F", "#E50914", "#000000"],
    category: "Popular Brands",
  },
  {
    name: "Starbucks",
    colors: ["#006341", "#000000", "#FFFFFF"],
    category: "Popular Brands",
  },
  {
    name: "nVidia",
    colors: ["#517C00", "#75B600", "#000000"],
    category: "Popular Brands",
  },
  {
    name: "Meta",
    colors: ["#0265E0", "#0080F9", "#1C2B33"],
    category: "Popular Brands",
  },
  {
    name: "Google",
    colors: ["#4285F4", "#EA4335", "#FBBC05", "#34A853"],
    category: "Popular Brands",
  },
  {
    name: "Olympics",
    colors: ["#0081C8", "#000000", "#DF3D59", "#F0BA6A", "#00A651"],
    category: "Popular Brands",
  },
];
