import { Button } from "@/components/ui/button";
import { Moon, Sun } from "lucide-react";
import type { FC } from "react";
import { useNavigate } from "react-router";
import { useTheme } from "./theme-provider";

const Header: FC = () => {
    const navigate = useNavigate();
    const { theme, setTheme } = useTheme();

    const changeTheme = () => {
        switch (theme) {
            case "dark": {
                setTheme("light");
                break;
            }
            case "light": {
                setTheme("dark");
                break;
            }
        }
    };

    return (
        <header className="relative flex items-center justify-between p-4 border-b">
            <div className="flex items-center">
                <img
                    src={"/favicon.svg"}
                    className="block h-10 w-auto mr-4 dark:invert-100"
                    alt="vscraper dark"
                />
                <div className="hidden md:block">
                    <img
                        src={"/vscraper-dark.svg"}
                        className="block dark:hidden h-8 w-auto mr-4"
                        alt="vscraper dark"
                    />
                    <img
                        src={"/vscraper-light.svg"}
                        className="dark:block hidden h-8 w-auto mr-4"
                        alt="vscraper dark"
                    />
                </div>
            </div>

            <div className="absolute left-1/2 -translate-x-1/2 flex space-x-1">
                <Button
                    variant="default"
                    onClick={() => {
                        navigate("/starter");
                    }}
                >
                    Home
                </Button>
                <Button
                    variant="default"
                    onClick={() => {
                        navigate("/");
                    }}
                >
                    Download
                </Button>
                <Button variant="default">Settings</Button>
            </div>

            <div>
                <Button
                    variant="default"
                    size="icon"
                    onClick={() => changeTheme()}
                >
                    <Sun className="h-[1.2rem] w-[1.2rem] rotate-0 scale-100 transition-all dark:-rotate-90 dark:scale-0" />
                    <Moon className="absolute h-[1.2rem] w-[1.2rem] rotate-90 scale-0 transition-all dark:rotate-0 dark:scale-100" />
                </Button>
            </div>
        </header>
    );
};

export default Header;
